FROM rust:1.94.1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y build-essential

WORKDIR /usr/src/paper

COPY Cargo.toml Cargo.lock default.pconf log4rs.yaml ./
COPY ./src ./src

RUN cargo build --release
RUN cargo install paper-cli

FROM debian:bookworm-slim

WORKDIR /usr/src/paper

COPY --from=builder /usr/src/paper/target/release/paper-server ./

# run the server
ENTRYPOINT ["/usr/src/paper/paper-server"]
