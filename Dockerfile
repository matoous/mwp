FROM rust:1.70 as builder

# deps
WORKDIR /app
RUN USER=root cargo init
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release

# app
ADD . ./
RUN rm ./target/release/deps/mwp*
RUN cargo build --release

# runtime
FROM debian:buster-slim
WORKDIR /app
ARG VERSION

RUN apt update \
    && apt install -y \
      ca-certificates \
      tzdata \
    && rm -rf /var/lib/apt/lists/*

ENV TZ=Etc/UTC

COPY --from=builder /app/target/release/mwp mwp

EXPOSE 4444

CMD ["/app/mwp"]
