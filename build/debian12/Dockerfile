FROM rust:1.85.1-bookworm

RUN apt-get update \
 && apt-get install -y --no-install-recommends binutils ca-certificates ripgrep \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /source
ENTRYPOINT ["./scripts/build-release.sh"]
