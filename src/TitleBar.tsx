import { useCallback, useEffect, useState } from "react"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { Minus, Maximize2, Minimize2, Pin, X } from "lucide-react"

/**
 * 自定义窗口标题栏。
 *
 * 使用 data-tauri-drag-region 支持拖拽移动窗口，并绘制与参考图一致的
 * 细线几何风格控制按钮：置顶、最小化、最大化/还原、关闭。
 */
export default function TitleBar() {
  const currentWindow = getCurrentWindow()
  const [isMaximized, setIsMaximized] = useState(false)
  const [isPinned, setIsPinned] = useState(false)

  /**
   * 同步窗口最大化状态。
   */
  const refreshMaximized = useCallback(async () => {
    try {
      const maximized = await currentWindow.isMaximized()
      setIsMaximized(maximized)
    } catch {
      // 在纯 Web 预览等环境中忽略错误
    }
  }, [currentWindow])

  /**
   * 初始化最大化状态并监听窗口尺寸变化。
   */
  useEffect(() => {
    let unlisten: (() => void) | undefined

    void refreshMaximized()
    currentWindow
      .onResized(() => {
        void refreshMaximized()
      })
      .then((cleanup) => {
        unlisten = cleanup
      })
      .catch(() => {})

    return () => {
      if (unlisten) {
        unlisten()
      }
    }
  }, [currentWindow, refreshMaximized])

  /**
   * 最小化窗口。
   */
  async function handleMinimize() {
    try {
      await currentWindow.minimize()
    } catch {
      // 忽略非 Tauri 环境
    }
  }

  /**
   * 最大化或还原窗口。
   */
  async function handleMaximize() {
    try {
      if (isMaximized) {
        await currentWindow.unmaximize()
      } else {
        await currentWindow.maximize()
      }
    } catch {
      // 忽略非 Tauri 环境
    }
  }

  /**
   * 关闭窗口。
   */
  async function handleClose() {
    try {
      await currentWindow.close()
    } catch {
      // 忽略非 Tauri 环境
    }
  }

  /**
   * 切换窗口置顶状态。
   */
  async function handleTogglePin() {
    try {
      const next = !isPinned
      await currentWindow.setAlwaysOnTop(next)
      setIsPinned(next)
    } catch {
      // 忽略非 Tauri 环境
    }
  }

  /**
   * 标题栏按下鼠标时启动窗口拖拽。
   *
   * 不使用 data-tauri-drag-region / -webkit-app-region: drag，
   * 因为该 CSS 属性会让操作系统接管拖拽区域的光标，导致自定义鼠标样式
   * 在标题栏消失且移出后不恢复。改用 start_dragging() API 在 mousedown
   * 时手动启动拖拽，标题栏不再是 drag region，CSS 光标可正常生效。
   *
   * 仅响应主键（左键）且不响应控件容器内的按下，避免影响按钮交互。
   * @param e 鼠标事件
   */
  async function handleTitlebarMouseDown(e: React.MouseEvent<HTMLElement>) {
    if (e.button !== 0) {
      return
    }
    const target = e.target as HTMLElement
    // 控件容器内的按下不触发拖拽（保留按钮点击）
    if (target.closest(".titlebar-controls")) {
      return
    }
    try {
      await currentWindow.startDragging()
    } catch {
      // 忽略非 Tauri 环境
    }
  }

  return (
    <header className="titlebar" onMouseDown={handleTitlebarMouseDown}>
      <div className="titlebar-brand">
        <span className="titlebar-brand-text">Mindustry Launcher</span>
      </div>
      <div className="titlebar-controls">
        <button
          className={`titlebar-button titlebar-pin ${isPinned ? "active" : ""}`}
          onClick={handleTogglePin}
          title={isPinned ? "取消置顶" : "置顶窗口"}
          aria-label={isPinned ? "取消置顶" : "置顶窗口"}
          type="button"
        >
          <Pin size={15} />
        </button>
        <button
          className="titlebar-button"
          onClick={handleMinimize}
          title="最小化"
          aria-label="最小化"
          type="button"
        >
          <Minus size={15} />
        </button>
        <button
          className="titlebar-button"
          onClick={handleMaximize}
          title={isMaximized ? "还原" : "最大化"}
          aria-label={isMaximized ? "还原" : "最大化"}
          type="button"
        >
          {isMaximized ? <Minimize2 size={15} /> : <Maximize2 size={15} />}
        </button>
        <button
          className="titlebar-button titlebar-close"
          onClick={handleClose}
          title="关闭"
          aria-label="关闭"
          type="button"
        >
          <X size={15} />
        </button>
      </div>
    </header>
  )
}


