# Dependency notices

The web build writes `THIRD-PARTY-NOTICES.txt` into `web/dist` from the installed, locked npm
and Cargo dependency trees. Keep that file with distributed web artifacts. It includes the
dependency license texts, the IBM Plex font licenses, and the Rust libraries linked into
the WASM engine. `web/license-notices.mjs` regenerates it during `npm run build`.

Project contributions are offered under the [MIT License](LICENSE), which is also copied into
the web distribution. These dependency notices do not grant rights in the original game or
user-imported resources; dependencies retain their own licenses.
