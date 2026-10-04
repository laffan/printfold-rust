/**
 * PrintFold - Booklet Creator
 * Main entry point
 */

import './styles/main.css';
import { App } from './components/App';
import { bridge } from './services/bridge';

// Initialize the application when DOM is ready
document.addEventListener('DOMContentLoaded', async () => {
  // Platform info (macOS / iPadOS) drives a few layout and dialog choices.
  await bridge.init();

  const app = new App();
  app.init();

  // Expose app for debugging in development
  if (import.meta.env.DEV) {
    (window as unknown as { printfoldApp: App }).printfoldApp = app;
  }
});
