FROM rust:1.46 as build
RUN rustup default nightly
WORKDIR /usr/src/app
COPY . .
RUN cargo build --package piarch-a-token-rs --release

CMD ["/usr/src/app/target/release/piarch-a-token-rs"]
EXPOSE 8000
