#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(cd -- "$script_dir/.." && pwd)"

image="${LINUX_CROSS_DOCKER_IMAGE:-quick-presenter-linux-cross:rust-1.96-bookworm}"
target="${LINUX_CROSS_TARGET:-x86_64-unknown-linux-gnu}"
rust_image="${LINUX_CROSS_RUST_IMAGE:-rust:1.96-bookworm}"
dockerfile="$repo_dir/docker/linux-cross/Dockerfile"
target_dir="${LINUX_CROSS_DOCKER_TARGET_DIR:-/target}"
registry_volume="${LINUX_CROSS_DOCKER_REGISTRY_VOLUME:-quick-presenter-linux-cross-cargo-registry}"
git_volume="${LINUX_CROSS_DOCKER_GIT_VOLUME:-quick-presenter-linux-cross-cargo-git}"
target_volume="${LINUX_CROSS_DOCKER_TARGET_VOLUME:-quick-presenter-linux-cross-target}"
build_args=()
cargo_args=(check --locked --target "$target")
rebuild=0

usage() {
  cat >&2 <<'EOF'
Usage: scripts/check_linux_cross_docker.sh [--rebuild] [-- <cargo args>]

Environment variables:
  LINUX_CROSS_DOCKER_IMAGE       Docker image tag to build or use.
                                 Default: quick-presenter-linux-cross:rust-1.96-bookworm
  LINUX_CROSS_RUST_IMAGE         Base Rust image.
                                 Default: rust:1.96-bookworm
  LINUX_CROSS_TARGET             Rust target triple.
                                 Default: x86_64-unknown-linux-gnu
  LINUX_CROSS_DOCKER_TARGET_DIR  Container Cargo target dir.
                                 Default: /target
  LINUX_CROSS_DOCKER_REGISTRY_VOLUME
                                 Docker volume for Cargo registry cache.
  LINUX_CROSS_DOCKER_GIT_VOLUME  Docker volume for Cargo git cache.
  LINUX_CROSS_DOCKER_TARGET_VOLUME
                                 Docker volume for Cargo build output.

The default command is:
  cargo check --locked --target x86_64-unknown-linux-gnu

Pass custom Cargo arguments after --, for example:
  scripts/check_linux_cross_docker.sh -- test --locked --target x86_64-unknown-linux-gnu
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --rebuild)
      build_args+=(--no-cache)
      rebuild=1
      shift
      ;;
    --)
      shift
      cargo_args=("$@")
      break
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker is required for the Linux cross check." >&2
  exit 1
fi

if [[ "$rebuild" -eq 1 ]] || ! docker image inspect "$image" >/dev/null 2>&1; then
  build_command=(docker build \
    --platform linux/amd64 \
    --file "$dockerfile" \
    --build-arg "RUST_IMAGE=$rust_image" \
    --tag "$image" \
    "$repo_dir")
  if [[ ${#build_args[@]} -gt 0 ]]; then
    build_command=("${build_command[@]:0:${#build_command[@]}-1}" "${build_args[@]}" "$repo_dir")
  fi
  "${build_command[@]}"
fi

docker run --rm \
  --platform linux/amd64 \
  --volume "$repo_dir:/work:ro" \
  --volume "$registry_volume:/usr/local/cargo/registry" \
  --volume "$git_volume:/usr/local/cargo/git" \
  --volume "$target_volume:$target_dir" \
  --workdir /work \
  --env "CARGO_TARGET_DIR=$target_dir" \
  "$image" \
  cargo "${cargo_args[@]}"
