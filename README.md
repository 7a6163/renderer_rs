# renderer_rs

`renderer_rs` is a project written in Rust, aiming to port the [zenato/puppeteer-renderer](https://github.com/zenato/puppeteer-renderer) project. The currently implemented example is:

- Rendering web content via `http://localhost:8080/html?url=http://www.google.com`.

## Table of Contents

- [Background](#background)
- [Installation](#installation)
- [Usage](#usage)
- [Example](#example)
- [Contributing](#contributing)
- [License](#license)

## Background

The `renderer_rs` project aims to provide a web rendering service using Rust and Headless Chrome. This project ports the functionality of [zenato/puppeteer-renderer](https://github.com/zenato/puppeteer-renderer) and implements it in Rust to leverage Rust's performance and safety features.

## Installation

### Prerequisites

- Rust 1.56+
- Docker (optional, for containerized deployment)

### Clone the Repository

```bash
git clone https://github.com/yourusername/renderer_rs.git
cd renderer_rs
cargo build --release
```

## Docker Deployment

### Build and run the Docker container:

```bash
docker build -t renderer_rs .
docker run -p 8080:8080 renderer_rs
```

## Usage

### Once the service is running, you can access the rendering service via the following URL:

```
http://localhost:8080/html?url=http://www.google.com
```

This URL will render the Google homepage and return the HTML content.

## Contributing

Contributions are welcome! Please submit a pull request or create an issue to discuss.
