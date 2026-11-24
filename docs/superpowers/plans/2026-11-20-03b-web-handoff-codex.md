# 计划 3b：网页前端剩余工作移交给 Codex

日期：2026-11-20。执行者：Codex CLI（WSL，`gpt-6.2-astra`，reasoning high，完全权限，允许开 subagent）。用户要求前端剩余工作全部交给 Codex，自主做完，中途不需要汇报。

## 工作位置

- git worktree：`D:\MyProj\KVMPanel-web`，WSL 路径 `/mnt/d/MyProj/KVMPanel-web`，分支 `feat/web`。
- 应用目录：`software/app/web/`（Vite、React、TypeScript）。
- **在这个 worktree 里工作，不要在 `main` 上改网页代码。**可以用 `git merge main` 拿到最新的契约。

## 先读这些

1. 计划：`docs/superpowers/plans/2026-11-20-03-web-frontend.md`。其中的 Global Constraints 和 Review Focus 是硬性要求。
2. 进度台账：`.superpowers/sdd/2026-11-20-03-web-frontend/progress.md`（在 worktree 里）。台账里有每个任务的完成状态、所有 Ruling（已做的决定，照办），以及 `minor (deferred)`（推迟的小问题）。
3. 每个任务的 brief 和 report：`.superpowers/sdd/2026-11-20-03-web-frontend/task-N-*.md`。
4. 契约：`software/app/api/`（openapi.yaml、README.md、schema/、ISSUES.md 最后几节的裁定）。只读，不要改。发现问题时写进 `software/app/api/ISSUES.md`（这是唯一允许改的文件），再按最合理的理解先实现。
5. 设计：`docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md` 第 5 节；草图在 `D:\MyProj\KVMPanel\.superpowers\brainstorm\475-1795176879\content\kvm-web.html` 和 `kvm-files.html`。

## 当前状态

- Task 1–5：已完成，审查也通过了。
- Task 6（设置窗口）：主体已提交（`698a8fe`）。第 1 轮修复已提交（`619cd7b`），但修复它的 agent 在中途被停下，**据它说提交里有一个测试是失败的**；另外 `software/app/web/test/setup.ts` 里还有一处没提交的改动。你先要做的是：
  1. 看一下 `test/setup.ts` 的未提交改动，判断是保留还是丢弃，然后跑全量测试，把失败的测试修好。
  2. 确认第 1 轮修复的这几项都已经做到，并且都有测试：
     - USB 预算：在任何开关切换时都要先预测，包括关掉 `mouse_merge`。
     - 网络设置保存前要确认，取消时不发任何请求。
     - 离线升级在上传前要确认，确认后的 PUT 要带 `X-Sha256`。
     - 改自己密码时的标记：POST 成功之后才开始 5 秒计时。
     - `closeSettings` 也要清掉 `floatingOpen`。
     - 没有 `terminal` 权限时，隐藏 Input 页里的串口设置。
     - 设置窗口打开时自动获得焦点，Esc 能关掉。
  3. 新的端点规则：**只要没开绝对鼠标，遥测就占 1 个 IN**（`if (u.telemetry && !u.mouse_abs) inEp += 1`）。`src/settings/usbBudget.ts` 和它的测试要和 `software/app/api/mock/test/usb.test.ts` 逐条一致（先 `git merge main`）。
- Task 7（浮窗组件）、Task 8（文件/终端工作台）、Task 9（浏览器走查和构建）：还没开始，按计划实现。

## 怎么做

- 每个任务都按计划里的步骤，先写测试、确认它失败，再实现，然后跑测试确认通过。完成后提交，提交信息用英文，结尾加 `Co-Authored-By: Codex <noreply@openai.com>`。
- 每个任务完成后，在 `progress.md` 里追加一行，格式和已有的行相同；自己做的决定按 `Ruling:` 的格式记进去。
- 每个任务做完，开一个 subagent 按计划审查，范围包括规格和代码质量；有 Critical 或 Important 问题就修，修完再复查。
- 全部任务完成后：
  1. 再 `git merge main` 一次，重新生成类型（`npm run gen:api && npm run gen:ws`）。
  2. 对整个分支做一次整体审查：从 `main` 和 `feat/web` 的分叉点到 HEAD。审查时要逐条看台账里的 `minor (deferred)`，影响用户的就修掉。
  3. 跑全量测试和 `npm run build`。
  4. 启动 `kvmd --sim`（命令见 `software/app/ipkvm/README.md`）或者 mock，在浏览器里走一遍 Task 9：zh 和 en 都要看；无头浏览器可以用 Playwright，但不要往系统里安装东西。
- **不要合并到 `main`，不要 push。**合并由 Claude 和用户决定。
- 不要改 `software/app/api/`（`ISSUES.md` 除外）、`software/app/ipkvm/`、`software/app/dashboard/`。

## 最终报告

用中文写，内容包括：
- 每个任务的提交范围，以及审查结论。
- 全量测试和构建的结果。
- 浏览器走查中发现并修掉的问题。
- 剩下的、需要用户或 Claude 决定的事项。
- `ISSUES.md` 里新增的契约问题。
