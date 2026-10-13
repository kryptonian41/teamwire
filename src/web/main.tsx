import { createRoot } from "react-dom/client";
import { App } from "./App.tsx";
import { selectClient } from "./api.ts";

const root = document.getElementById("root");
if (root) createRoot(root).render(<App client={selectClient()} />);
