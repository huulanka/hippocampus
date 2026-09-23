# client

The desktop app: a Tauri v2 shell (`src-tauri/`, Rust) around a React and
TypeScript interface (`src/`), built with Vite.

`src-tauri/` is its own Cargo workspace, separate from the backend's — run
Cargo commands for it from inside that directory. Why, and how to run the
app from source without disturbing the installed one:
[`docs/development.md`](../docs/development.md).

```sh
npm install
npm run dev:app   # the desktop app, under its own bundle identity
npm run build     # the interface alone, type-checked
```
