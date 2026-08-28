FROM rust:1.97-bookworm AS builder 

WORKDIR app/ 
COPY src/ ./src/ 
COPY Cargo.lock Cargo.toml ./ 
COPY pages/ ./pages/ 

RUN cargo build --release 

FROM debian:bookworm-slim 
COPY --from=builder app/target/release/msggram usr/local/bin/msggram 
COPY --from=builder app/pages/ /app/pages/ 

WORKDIR app/ 

CMD ["msggram"]
