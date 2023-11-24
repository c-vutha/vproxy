FROM rust:1-slim as builder
RUN USER=root cargo new --bin vproxy
WORKDIR /vproxy
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release && rm src/*.rs
COPY ./src ./src
RUN rm -rf ./target/release/deps/vproxy* && cargo build --release


FROM debian:stable-slim
WORKDIR /app
COPY --from=builder /vproxy/target/release/vproxy ./vproxy
CMD ["./vproxy"]