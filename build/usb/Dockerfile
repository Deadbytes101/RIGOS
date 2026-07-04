FROM debian:12-slim

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      ca-certificates curl git isolinux live-build squashfs-tools \
      syslinux-common syslinux-utils xorriso \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /source
ENTRYPOINT ["./scripts/build-usb-image.sh"]
