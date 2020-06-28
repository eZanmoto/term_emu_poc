# Copyright 2020 Sean Kelleher. All rights reserved.
# Use of this source code is governed by an MIT
# licence that can be found in the LICENCE file.

build_img=rust:1.40.0-stretch

# We use named volumes to keep the Rust caches between builds. We make them
# writable by anyone because we run the build as the local user (`id -u` and
# `id -g`), and the volumes are owned by root by default.
docker run \
    --rm \
    --mount='type=volume,src=term_emu_cargo_cache,dst=/cargo' \
    $build_img \
    chmod 0777 /cargo

# This command is run using `--interactive` and `--tty` so that it can be killed
# using `^C`. Note that these flags would need to be removed in order to run
# this script in a continuous integration environment.
docker run \
    --interactive \
    --tty \
    --rm \
    --user=$(id -u):$(id -g) \
    --mount='type=volume,src=term_emu_cargo_cache,dst=/cargo' \
    --env='CARGO_HOME=/cargo' \
    --user="$(id -u):$(id -g)" \
    --mount="type=bind,src=$(pwd),dst=/app" \
    --workdir='/app' \
    $build_img \
    cargo build
