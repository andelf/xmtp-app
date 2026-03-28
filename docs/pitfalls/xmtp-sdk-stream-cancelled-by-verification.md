# XMTP SDK: streamAllMessages 被验证代码意外取消

## 问题

会话列表页的 lastMessage 预览永远不更新（显示 "No messages yet"），但进入聊天详情页能看到消息。手动下拉刷新后预览才正常。

## 根因

auth store 的 `init()` 中有一段 SDK 可行性验证代码：

```typescript
// 验证 streaming 是否工作
await result.client.conversations.streamAllMessages(async (msg) => {
  console.log("[XMTP][Stream] New message:", msg.id);
});
// 5 秒后取消
setTimeout(() => {
  result.client.conversations.cancelStreamAllMessages();
}, 5000);
```

与此同时，`useConversations` hook 在 `_layout.tsx` 挂载时也注册了 `streamAllMessages`。

**`cancelStreamAllMessages()` 是按 `installationId` 全局取消的**——它取消的不是"谁注册的就取消谁的"，而是取消该 client 上所有的 allMessages stream 订阅。所以 5 秒后验证代码的 `cancel` 把 `useConversations` 的全局消息 stream 也一起杀掉了。

这就是为什么：
- 前 5 秒内可能偶尔能收到消息
- 5 秒后彻底收不到
- 聊天详情页不受影响（它用的是 per-conversation 的 `streamMessages`，不同的事件类型 `ConversationMessage` vs `Message`）

## 修复

删除 auth store 中的 SDK 验证代码。验证应该在开发阶段一次性完成，不应该留在生产代码中。

## 教训

1. **XMTP SDK 的 cancel 方法是全局的**：`cancelStreamAllMessages()` / `cancelStream()` 取消的是该 client 实例上所有同类订阅，不区分调用者。不要在多个地方对同一个 client 注册+取消同类 stream。

2. **调试 stream 类问题的有效方法**：
   - Release build 的 Hermes `.hbc` bundle 会 strip 掉 `console.log`
   - 设置 `debuggable true` 后 `adb shell run-as` 可以访问 app 私有文件
   - 但 `console.log` 仍然不可见，因为 bundle 是 production 编译的
   - **真正有效的方法**：用 `npx expo export --platform android --dev` 生成 dev bundle（`.js` 而非 `.hbc`），然后复制到 assets 目录。这样 `console.log` 在 `adb logcat -s ReactNativeJS` 中可见
   - 完整命令：
     ```bash
     npx expo export --platform android --dev
     cp dist/_expo/static/js/android/*.js android/app/src/main/assets/index.android.bundle
     cd android && ./gradlew assembleRelease
     adb install -r app/build/outputs/apk/release/app-release.apk
     adb logcat -s "ReactNativeJS:*"
     ```

3. **stream 不工作时的排查思路**：
   - 先确认 stream 回调是否被调用（加日志）
   - 如果完全没被调用，检查是否有其他地方 cancel 了同类 stream
   - XMTP SDK 有两套独立的消息事件：`EventTypes.Message`（streamAllMessages）和 `EventTypes.ConversationMessage`（per-conversation streamMessages），互不影响
   - `stream()` 的 `onClose` 回调可以检测 stream 断开

4. **Android 厂商电池优化**：OPPO/OnePlus/Realme 的 HansManager 会在 app 切后台后立即冻结进程（`freeze uid`），导致所有 JS 线程暂停。这不是 stream 的问题，而是进程级别的挂起。前台运行不受影响。
