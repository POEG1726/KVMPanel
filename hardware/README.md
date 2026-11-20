# 硬件

嘉立创 EDA 中当前工程仍叫 `OC_Controller`，板子叫 `Monitor`；本项目统一叫 KVMPanel。

- `eda/`：留给原生工程文件，目前尚未成功导出。
- `exports/`：后续放原理图、网表、BOM 等。
- `reference/`：现有芯片手册和 PCB 器件清单。

`reference/datasheets/` 中的手册复制自原 JLCEDA 工作区，原文件保留。

`reference/board-components-20261120.json` 是当日打开 PCB 的器件属性清单，不代表实物已装配，也不能当网表使用。

原生 EDA 工程导出提示“未启用扩展和独立脚本的外部交互权限”，目前未取得完整工程备份。后续拿到原生工程后放进 `eda/`。
