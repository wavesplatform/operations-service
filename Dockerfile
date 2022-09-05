FROM rust:1.63 as builder
WORKDIR /usr/src/service

RUN rustup component add rustfmt

COPY Cargo.* ./
COPY ./src ./src
COPY ./migrations ./migrations

RUN cargo install --path .


FROM debian:11 as runtime
WORKDIR /usr/www/app

RUN apt-get update && apt-get install -y curl openssl libssl-dev libpq-dev procps net-tools curl
# RUN curl -ks 'https://cert.host.server/ssl_certs/EnterpriseRootCA.crt' -o '/usr/local/share/ca-certificates/EnterpriseRootCA.crt'
RUN /usr/sbin/update-ca-certificates

COPY --from=builder /usr/local/cargo/bin/api .
COPY --from=builder /usr/local/cargo/bin/migration .
COPY --from=builder /usr/local/cargo/bin/consumer .

COPY --from=builder /usr/src/service/migrations ./migrations/ 

CMD ["./api"]
