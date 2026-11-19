import { mount } from "svelte";

import Cards from "./Cards.svelte";
import "./styles/app.css";

const target = document.getElementById("app");
if (target) mount(Cards, { target });
