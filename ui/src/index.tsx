/* @refresh reload */
import { render } from "solid-js/web";

import App from "./App";
import "./styles/app.css";
import "./styles/dashboard.css";

const root = document.getElementById("root");
if (root) {
  render(() => <App />, root);
}
