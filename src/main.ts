import { createApp } from "vue";
import { Quasar, Notify } from "quasar";
import App from "./App.vue";
import "@quasar/extras/material-symbols-rounded/material-symbols-rounded.css";
import "quasar/src/css/index.sass";
import "./app.css";

import iconSet from "quasar/icon-set/material-symbols-rounded";

createApp(App).use(Quasar, {
  plugins: {
    Notify,
  },
  config: {
    notify: {
      position: "bottom-right",
      timeout: 2500,
    },
  },
  iconSet: iconSet,
}).mount("#app");
