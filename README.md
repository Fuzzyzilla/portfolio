# [Portfolio](https://fuzzyzilla.github.io)
A site with WASM code based on [wasm-pack-template](https://github.com/rustwasm/wasm-pack-template) for the `vector-sheepy` crate
and [`create-wasm-app`](https://github.com/rustwasm/create-wasm-app) for the `www` portion.

## Publishing
1) Execute `wasm-pack build` from within the `vector-sheepy` crate to build the wasm binary used by `www`.
2) To preview, execute `npm run start` from within the `www` directory and direct your browser to `localhost:8080`.
3) To publish, execute `npm run build`, and finished artifacts will be placed into the `www/dist` directory.
