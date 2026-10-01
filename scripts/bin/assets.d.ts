// Bun's `with { type: 'file' }` imports resolve to the embedded file's path.
declare module '*.wasm' {
  const path: string;
  export default path;
}
