# WSR browser engine

## Layout

- `engine/wsr-core`: numeric runtime, game state, compatible logic, save reader/writer and resource loader.
- `engine/wsr-wasm`: session binding and browser ABI.
- `web`: Svelte interface, worker, local installer reader and extraction manifest.
- `tools`: independently written analysis and validation utilities; private binary inputs are required for original-code comparisons.

See [engine notes](engine/README.md) for compatibility conventions and [browser notes](web/README.md) for hosting details.

## Disclaimer

This is an unofficial fan project for Wall Street Raider. It is not affiliated with or
endorsed by the game's creators, publishers, or rights holders. The game's name, branding,
and resources remain the property of their respective owners.

No original game assets or extracted game content are bundled with this repository or its
web distribution. You must supply your own lawfully obtained copy of the supported game
installer. The project's MIT License covers its authors' contributions and grants no rights
in the original game or user-imported resources.
