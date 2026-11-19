import { mount } from "svelte";

import Projects from "./Projects.svelte";
import "./styles/app.css";

const target = document.getElementById("app");
if (target) mount(Projects, { target });
