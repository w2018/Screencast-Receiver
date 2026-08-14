// 全局 Toast 通知组件（F10）：操作反馈，3 秒自动消失
import { useToastStore } from "../../stores/toastStore";

export function Toast() {
  const message = useToastStore((s) => s.message);
  if (!message) return null;
  return (
    <div className="toast" role="status">
      {message}
    </div>
  );
}
