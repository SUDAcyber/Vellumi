import { mount } from "svelte";

import Settings from "./Settings.svelte";
import "./styles/app.css";

const target = document.getElementById("app");
if (target) mount(Settings, { target });
