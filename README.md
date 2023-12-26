# Piarch-a-token-rs Documentation

## Overview

This Rust project provides a simple authentication system using JSON Web Tokens (JWT) and MongoDB. It includes functionalities for user login, token creation, and token validation.

## Table of Contents

- [Setup](#setup)
    - [Prerequisites](#prerequisites)
    - [Installation](#installation)
- [Usage](#usage)
    - [Initialize Database](#initialize-database)
    - [Create Token](#create-token)
    - [Validate User](#validate-user)
    - [Evaluate Credentials](#evaluate-credentials)
    - [Token Retrieval](#token-retrieval)
- [Endpoints](#endpoints)
    - [/login](#login-endpoint)
- [Configuration](#configuration)
- [Contributing](#contributing)
- [License](#license)

## Setup

### Prerequisites

- Rust installed. (See [rustup](https://rustup.rs/) for installation instructions.)
- MongoDB server.
- [dotenv](https://crates.io/crates/dotenv) and [mongodb](https://crates.io/crates/mongodb) Rust crates.

### Installation

1. Clone the repository:

   ```bash
   git clone https://github.com/dogabudak/piarch-a-token-rs.git

2. Build the project:

    ```bash
    cargo build

3. Run the application:

    ```bash
    cargo run

    

