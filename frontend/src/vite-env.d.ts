/// <reference types="vite/client" />

declare module "*.arb?raw" {
  const value: string;
  export default value;
}

interface Window {
  __TAURI_INTERNALS__?: unknown;
}
