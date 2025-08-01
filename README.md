# Cubiq Blockchain – Mobile-Native Layer 1

This project is a reference implementation of a mobile-native blockchain as described by Cubiq Network. Structure and components are modular for mobile, cloud, and EVM compatibility.

## Structure

- core/        — Rust core (zkURL, consensus, prover, networking)
- contracts/   — EVM contracts (Solidity, zkEVM)
- mobile/      — Client SDKs and native bridges
- cloud/       — Prover network and API gateway
- tests/       — Unit, integration, e2e tests
- docs/        — Architecture and API docs

## Quickstart

- Requires Rust, Node.js, React Native CLI
- See docs/ for build & run instructions

See `.env.development` for environment variables.


## License

Cubic Chain is licensed under the [Business Source License 1.1](LICENSE.md).
Use is restricted to non-commercial purposes only.
