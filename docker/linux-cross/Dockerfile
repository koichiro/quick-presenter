ARG RUST_IMAGE=rust:1.96-bookworm
FROM ${RUST_IMAGE}

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libegl1-mesa-dev \
        libfontconfig1-dev \
        libgl1-mesa-dev \
        libudev-dev \
        libwayland-dev \
        libx11-dev \
        libxcb1-dev \
        libxkbcommon-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-gnu

WORKDIR /work
