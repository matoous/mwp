FROM rust:1.85.1-slim as builder

RUN apt update \
  && apt install -y libssl-dev pkg-config
RUN rustup update

# deps
WORKDIR /app

COPY . ./
RUN cargo build --release --bin mwp

# runtime
FROM debian:bookworm-slim
WORKDIR /app
ARG VERSION

RUN apt update \
    && apt install -y \
      pkg-config \
      libssl-dev \
      ca-certificates \
      tzdata \
    && rm -rf /var/lib/apt/lists/*

ENV TZ=Etc/UTC

COPY --from=builder /app/target/release/mwp mwp
COPY ./db.db3 ./
COPY ./wiki ./wiki

EXPOSE 4444

CMD ["/app/mwp", "--adr", "0.0.0.0:4444"]
