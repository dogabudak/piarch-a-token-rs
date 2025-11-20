#!/usr/bin/env node
/**
 * Test script to verify StatsD counters are working in the Rust token service.
 * This script will:
 * 1. Start a mock StatsD server to capture metrics
 * 2. Make test requests to your service
 * 3. Display the captured metrics
 */

const dgram = require('dgram');
const http = require('http');
const { setTimeout } = require('timers/promises');

class MockStatsD {
    constructor(host = '127.0.0.1', port = 8125) {
        this.host = host;
        this.port = port;
        this.metrics = {};
        this.server = dgram.createSocket('udp4');
        this.running = false;
    }

    start() {
        return new Promise((resolve) => {
            this.server.on('message', (msg, rinfo) => {
                const metricLine = msg.toString().trim();
                this.parseMetric(metricLine);
            });

            this.server.on('listening', () => {
                const address = this.server.address();
                console.log(`🎯 Mock StatsD server started on ${address.address}:${address.port}`);
                this.running = true;
                resolve();
            });

            this.server.bind(this.port, this.host);
        });
    }

    parseMetric(line) {
        // Parse StatsD metric line like 'piarch_token_service.requests.total:1|c'
        if (line.includes('|c')) { // Counter metric
            const parts = line.split(':');
            const metricName = parts[0];
            const value = parseFloat(parts[1].split('|')[0]);
            
            this.metrics[metricName] = (this.metrics[metricName] || 0) + value;
            console.log(`📊 Received metric: ${metricName} = ${value}`);
        }
    }

    stop() {
        if (this.running) {
            this.server.close();
            this.running = false;
        }
    }

    getMetrics() {
        return { ...this.metrics };
    }
}

function makeRequest(path, headers = {}) {
    return new Promise((resolve, reject) => {
        const options = {
            hostname: '127.0.0.1',
            port: 8000,
            path,
            method: 'GET',
            headers,
            timeout: 5000
        };

        const req = http.request(options, (res) => {
            let data = '';
            res.on('data', (chunk) => data += chunk);
            res.on('end', () => {
                resolve({
                    statusCode: res.statusCode,
                    data: data
                });
            });
        });

        req.on('error', (err) => {
            reject(err);
        });

        req.on('timeout', () => {
            req.destroy();
            reject(new Error('Request timeout'));
        });

        req.end();
    });
}

async function testRequests() {
    console.log('\n🚀 Starting request tests...');

    // Test 1: Request without authorization header (should increment unauthorized)
    console.log('\n1️⃣  Testing request without authorization header...');
    try {
        const response = await makeRequest('/login');
        console.log(`   Status: ${response.statusCode}`);
    } catch (error) {
        console.log(`   Error: ${error.message}`);
    }

    await setTimeout(500);

    // Test 2: Request with invalid authorization format (should increment failed)
    console.log('\n2️⃣  Testing request with invalid authorization...');
    try {
        const response = await makeRequest('/login', { 'authorize': 'invalid_format' });
        console.log(`   Status: ${response.statusCode}`);
    } catch (error) {
        console.log(`   Error: ${error.message}`);
    }

    await setTimeout(500);

    // Test 3: Request with proper format but invalid credentials (should increment failed)
    console.log('\n3️⃣  Testing request with invalid credentials...');
    try {
        const response = await makeRequest('/login', { 'authorize': 'Basic testuser:wrongpass' });
        console.log(`   Status: ${response.statusCode}`);
    } catch (error) {
        console.log(`   Error: ${error.message}`);
    }

    await setTimeout(500);

    // Test 4: Request with skeleton key - successful authentication (should increment success)
    console.log('\n4️⃣  Testing request with skeleton key (testuser:testpass)...');
    try {
        const response = await makeRequest('/login', { 'authorize': 'Basic testuser:testpass' });
        console.log(`   Status: ${response.statusCode}`);
        if (response.statusCode === 200) {
            console.log(`   ✅ Success! Received JWT token (${response.data.length} chars)`);
        }
    } catch (error) {
        console.log(`   Error: ${error.message}`);
    }

    await setTimeout(500);

    // Test 5: Multiple requests to test total counter
    console.log('\n5️⃣  Making multiple requests to test counters...');
    for (let i = 0; i < 3; i++) {
        try {
            const response = await makeRequest('/login', { 'authorize': `Basic user${i}:pass${i}` });
            console.log(`   Request ${i + 1}: Status ${response.statusCode}`);
        } catch (error) {
            console.log(`   Request ${i + 1}: Error ${error.message}`);
        }
        await setTimeout(200);
    }
}

async function checkServiceHealth() {
    try {
        await makeRequest('/login');
        return true;
    } catch (error) {
        return false;
    }
}

async function main() {
    console.log('🧪 StatsD Counter Test Script (Node.js)');
    console.log('='.repeat(50));
    console.log('This script will test your Rust service\'s StatsD integration.');
    console.log('Make sure your Rust service is running on http://127.0.0.1:8000');
    console.log();

    // Check if service is running
    const serviceRunning = await checkServiceHealth();
    if (!serviceRunning) {
        console.log('❌ Service not reachable at http://127.0.0.1:8000');
        console.log('Please start your Rust service first:');
        console.log('   cargo run');
        console.log('   (or if you have MONGODB env var issues, export MONGODB=your_connection_string)');
        process.exit(1);
    }

    console.log('✅ Service is reachable');

    // Start mock StatsD server
    const statsd = new MockStatsD();
    
    try {
        await statsd.start();
        await setTimeout(1000); // Give StatsD server time to start

        // Run the tests
        await testRequests();

        // Wait a bit for metrics to be captured
        await setTimeout(2000);

        // Display results
        console.log('\n📈 RESULTS');
        console.log('='.repeat(30));
        const metrics = statsd.getMetrics();

        if (Object.keys(metrics).length === 0) {
            console.log('❌ No metrics received!');
            console.log('   Check if your service is properly sending StatsD metrics.');
        } else {
            console.log('✅ Metrics captured:');
            for (const [metricName, count] of Object.entries(metrics)) {
                console.log(`   ${metricName}: ${count}`);
            }

            // Validate expected metrics
            const expectedMetrics = [
                'piarch_token_service.requests.total',
                'piarch_token_service.requests.unauthorized',
                'piarch_token_service.requests.failed'
            ];

            const missingMetrics = expectedMetrics.filter(m => !(m in metrics));
            if (missingMetrics.length > 0) {
                console.log(`\n⚠️  Missing expected metrics: ${missingMetrics.join(', ')}`);
            } else {
                console.log('\n✅ All expected metric types captured!');
            }

            // Check if total equals sum of others
            const total = metrics['piarch_token_service.requests.total'] || 0;
            const success = metrics['piarch_token_service.requests.success'] || 0;
            const failed = metrics['piarch_token_service.requests.failed'] || 0;
            const unauthorized = metrics['piarch_token_service.requests.unauthorized'] || 0;

            const expectedTotal = success + failed + unauthorized;
            console.log('\n🧮 Metric validation:');
            console.log(`   Total requests: ${total}`);
            console.log(`   Success + Failed + Unauthorized: ${expectedTotal}`);

            if (total === expectedTotal) {
                console.log('✅ Metric counts are consistent!');
            } else {
                console.log('⚠️  Metric counts don\'t add up - check your implementation');
            }
        }

    } catch (error) {
        console.error('❌ Test failed:', error.message);
    } finally {
        statsd.stop();
        console.log('\n🏁 Test completed');
        process.exit(0);
    }
}

// Handle Ctrl+C gracefully
process.on('SIGINT', () => {
    console.log('\n\n⏹️  Test interrupted by user');
    process.exit(0);
});

if (require.main === module) {
    main().catch(console.error);
}