# Operations service


## Description

The service unifies Waves and Ethereum transactions under a new term 'operations'.

Consists of a consumer and a web-service, plus a database migration tool.


## Build & run

> `cargo build --release`


### Migration tool

List of pending migrations:
> `cargo run --release --bin migration -- list`

Apply pending migrations:
> `cargo run --release --bin migration -- up`

Revert last migration:
> `cargo run --release --bin migration -- down`


### Consumer

> `cargo run --release --bin consumer`


### Web-service

> `cargo run --release --bin service`


## Environment variables


### Consumer

* `RUST_LOG` - logging parameters, as a start `debug,hyper=warn,h2=warn,tower=warn` is good enough
* `RUST_LOG_FORMAT` - log format, either `plain` or `json`, default `json`
* `BLOCKCHAIN_UPDATES_URL` - for mainnet this is `https://blockchain-updates.waves.exchange`
* `STARTING_HEIGHT` - starting blockchain height, for mainnet 1610030 is perfect, the very first `InvokeScript` transaction is at this height
* `BATCH_MAX_DELAY_SEC` - maximum interval between database writes, default 10 seconds
* `BATCH_MAX_SIZE` - maximum number of updates to batch, default 256
* `PGHOST` - Postgres host
* `PGUSER` - Postgres user
* `PGPASSWORD` - Postgres password
* `PGDATABASE` - postgres database name


### Web-service

* `RUST_LOG` - logging parameters, as a start `debug,hyper=warn,warp=warn` is good enough
* `RUST_LOG_FORMAT` - log format, either `plain` or `json`, default `json`
* `PORT` - web server port, default 8080
* `PGHOST` - Postgres host
* `PGUSER` - Postgres user
* `PGPASSWORD` - Postgres password
* `PGDATABASE` - postgres database name
* `PGPOOLSIZE` - database pool size, default 4


### Migrator

* `PGHOST` - Postgres host
* `PGUSER` - Postgres user
* `PGPASSWORD` - Postgres password
* `PGDATABASE` - postgres database name


## Usage

Create new empty database. Then run migrator once. Start consumer, then start web-service.

`http://localhost:8080/operations?sender=address&limit=10&after=...`