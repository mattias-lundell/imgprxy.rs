FROM rustlang/rust:nightly as build

# Creates a dummy project used to grab dependencies
RUN USER=root cargo new --bin dummy
WORKDIR /dummy

# Copies over *only* your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# Builds your dependencies and removes the
# fake source code from the dummy project
RUN cargo build --release
RUN rm src/*.rs
RUN rm target/release/imgprxy

# Copies only your actual source code to
# avoid invalidating the cache
COPY ./src ./src

# Builds again, this time it'll just be
# your actual source files being built
RUN cargo build --release

FROM rustlang/rust:nightly

# Copies the binary from the "build" stage to the current stage
COPY --from=build dummy/target/release/imgprxy .

# Configures the startup!
CMD ["./imgprxy"]