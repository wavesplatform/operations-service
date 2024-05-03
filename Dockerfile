FROM rust:1.75 as builder
WORKDIR /usr/src/service

RUN rustup component add rustfmt
RUN apt-get update && apt-get install -y protobuf-compiler

COPY Cargo.* ./
COPY ./src ./src
COPY ./migrations ./migrations

RUN cargo build --release


FROM debian:12 as runtime
WORKDIR /app

RUN apt-get update && apt-get install -y curl openssl libssl-dev libpq-dev procps net-tools curl postgresql-client
# RUN curl -ks 'https://cert.host.server/ssl_certs/EnterpriseRootCA.crt' -o '/usr/local/share/ca-certificates/EnterpriseRootCA.crt'
RUN /usr/sbin/update-ca-certificates

COPY --from=builder /usr/src/service/target/release/service .
COPY --from=builder /usr/src/service/target/release/migration .
COPY --from=builder /usr/src/service/target/release/consumer .

COPY --from=builder /usr/src/service/migrations ./migrations/ 

CMD ["./service"]
