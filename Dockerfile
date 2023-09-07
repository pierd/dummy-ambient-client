FROM rust:1.70-bullseye AS builder
ADD . /build
WORKDIR /build
RUN cargo build --release
RUN strip target/release/dummy-ambient-client

FROM debian:bullseye-slim
WORKDIR /app
COPY --from=builder /build/target/release/dummy-ambient-client ./
CMD [ "./dummy-ambient-client", "localhost:9000" ]
