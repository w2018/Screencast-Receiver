import ReactDOM from "react-dom/client";
import App from "./App";

// 注意：不使用 StrictMode —— mpv 插件管理的是原生单实例，双调用会导致
// mpv 实例重复初始化/销毁的竞态（仅 dev 环境出现）
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <App />,
);
