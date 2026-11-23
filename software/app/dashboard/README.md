# Dashboard

PC 机箱副屏界面，消费公共状态与遥测服务。LVGL、Slint 和 Qt Quick 的桌面原型分别在 `feat/panel-proto`、`feat/panel-slint` 和 `feat/panel-qt`。板端框架、横屏主屏和屏幕方向设置待实现。

电脑离线页已在 LVGL 和 Qt 原型中实现，竖屏和横屏各一版。数据来自 panel socket 的 `board` 消息，开机按钮发 `pc_power_on`。构建、运行和截图方法见各分支的 `prototypes/lvgl/NOTES.md` 与 `prototypes/qt/NOTES.md`，截图在 `prototypes/screens/`。调试时用 `KVMPANEL_MOCK_PC=off` 或 `waiting` 启动 Mock。 [软件进度](../../../docs/software-progress.md)
