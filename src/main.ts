import "./app.css";
import "highlight.js/styles/github.css"; // token colours for code previews (CPE-065)
import App from "./App.svelte";

const app = new App({
  target: document.getElementById("app")!,
});

export default app;
