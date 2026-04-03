# Nexa CLI

Official Docker image for the [Nexa language](https://nexa-lang.org) toolchain.

## Quick start

```bash
# Build a Nexa project
docker run --rm -v $(pwd):/app nexalang/nexa build

# Start the dev server on port 3000
docker run --rm -p 3000:3000 -v $(pwd):/app nexalang/nexa run

# Package a project into a .nexa bundle
docker run --rm -v $(pwd):/app nexalang/nexa package

# Publish to the registry
docker run --rm -v $(pwd):/app \
  -e NEXA_TOKEN=nxt_your_token \
  nexalang/nexa publish
```

## Available tags

| Tag | Description |
|-----|-------------|
| `latest` | Latest stable build from `main` |
| `1.x.y` | Specific release version |
| `sha-xxxxxxx` | Exact commit SHA |

## Usage with a shell alias

```bash
alias nexa='docker run --rm -v $(pwd):/app nexalang/nexa'

nexa init my-project
nexa build
nexa run
```

## Environment variables

| Variable | Description |
|----------|-------------|
| `NEXA_TOKEN` | API token for publishing (`nexa publish`) |
| `NEXA_REGISTRY` | Custom registry URL (default: `https://registry.nexa-lang.org`) |

## Install the CLI natively

If you prefer a native install:

```bash
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/nexa-lang-org/nexa-lang/main/setup.sh \
  | sh
```

## Links

- [GitHub](https://github.com/nexa-lang-org/nexa-lang)
- [Registry](https://registry.nexa-lang.org)
- [Documentation](https://nexa-lang.org/docs)
