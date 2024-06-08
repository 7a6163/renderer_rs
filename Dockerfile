# Build stage
FROM rust:1-alpine as builder

RUN apk add --no-cache build-base

WORKDIR /usr/src/renderer_rs
COPY .  ./

# Build dependencies
RUN cargo build --release

# Runtime stage
FROM ghcr.io/zenika/alpine-chrome

# Install required packages
RUN apk add --no-cache tini

# Copy the compiled binary from the build stage
COPY --from=builder /usr/src/renderer_rs/target/release/renderer_rs /usr/local/bin/renderer_rs

# Expose port 8080
EXPOSE 8080

ENTRYPOINT ["/sbin/tini", "--"]

# Run the application
CMD ["renderer_rs"]
