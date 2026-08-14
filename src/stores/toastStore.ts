// 全局 Toast 通知状态（F10 操作反馈）
import { create } from "zustand";

interface ToastState {
  message: string | null;
  /** 显示一条 3 秒自动消失的通知 */
  show: (msg: string) => void;
}

export const useToastStore = create<ToastState>((set) => ({
  message: null,
  show: (msg) => {
    set({ message: msg });
    // 3 秒后自动消失
    setTimeout(() => {
      set({ message: null });
    }, 3000);
  },
}));
