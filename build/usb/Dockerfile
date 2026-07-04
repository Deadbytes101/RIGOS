FROM debian:12-slim

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      ca-certificates curl git live-build xorriso squashfs-tools \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /source
ENTRYPOINT ["./scripts/build-usb-image.sh"]
