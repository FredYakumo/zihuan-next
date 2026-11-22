import { createApp } from "vue";
import TDesign from "tdesign-vue-next";
import "tdesign-vue-next/dist/tdesign.css";
import App from "./App.vue";
import "./style.scss";

createApp(App).use(TDesign).mount("#app");
