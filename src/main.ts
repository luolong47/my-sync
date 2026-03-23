import { createApp } from "vue";
import { Quasar, Notify } from "quasar";
import App from "./App.vue";
import "@quasar/extras/material-symbols-rounded/material-symbols-rounded.css";
import "quasar/src/css/index.sass";

createApp(App).use(Quasar, {
  plugins: {
    Notify,
  },
}).mount("#app");
