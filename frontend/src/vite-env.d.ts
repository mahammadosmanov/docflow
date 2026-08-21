/// <reference types="vite/client" />

declare const __DOCFLOW_VERSION__: string;

declare module "*.arb?raw" {
  const value: string;
  export default value;
}

interface Window {
  __TAURI_INTERNALS__?: unknown;
}
