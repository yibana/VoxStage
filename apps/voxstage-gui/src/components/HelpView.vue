<template>
  <div class="help-view">
    <section class="help-section">
      <h2>VoxStage 使用概览</h2>
      <p>
        VoxStage 是一个通过脚本（.vox）驱动的多角色 TTS 播放器，本 GUI 用来完成：
      </p>
      <ul>
        <li>配置 TTS 模型与角色</li>
        <li>以列表形式编辑 / 运行剧本</li>
        <li>在运行时高亮当前步骤，支持暂停 / 继续 / 中断 / 循环运行</li>
      </ul>
    </section>

    <section class="help-section">
      <h2>基本操作流程</h2>
      <ol>
        <li>
          在「配置」页：
          <ul>
            <li>添加模型（endpoint、provider、model_id 等）</li>
            <li>为每个模型添加一个或多个角色（params 可设置 language、speaker_id、emotion 等默认参数）</li>
          </ul>
        </li>
        <li>
          在「剧本」页：
          <ul>
            <li>用「添加步骤」按钮构建 speak / sleep / if / for / while / let / set / BGM 等语句</li>
            <li>使用表达式输入框右侧的变量 / 内置函数插入，快速编写条件和计算公式；输入时会提供简单的语法提示与自动补全</li>
            <li>在 speak 行上点击「参数」按钮，为单句台词覆写角色参数（如 language、text_lang、ref_audio_path 等）</li>
            <li>必要时切换到 Code 模式直接编辑 .vox 文本，再「返回编辑」同步回列表</li>
          </ul>
        </li>
        <li>
          运行剧本：
          <ul>
            <li>点击「运行」按钮启动脚本（可选勾选「循环」反复播放）</li>
            <li>运行中可随时「暂停 / 继续」或「中断」</li>
            <li>当前执行的 speak / sleep 行会在列表中高亮显示</li>
          </ul>
        </li>
      </ol>
    </section>

    <section class="help-section">
      <h2>.vox 剧本语法速查</h2>

      <h3>1. 模型与角色</h3>
      <pre class="code">
model GPT {
  type = "http"
  provider = "gpt_sovits_v2"
  endpoint = "http://127.0.0.1:9880"
  model_id = "0"
}

role Narrator {
  model = "GPT"
  language = "zh"
  speaker_id = "0"
}
      </pre>

      <h3>2. 变量与表达式</h3>
      <pre class="code">
let score = 100
set score = score + 1

if score &gt;= 60 {
  speak Narrator "及格啦，分数：${score}"
}

for 3 {
  speak Narrator "循环里的第 ${i} 次"
}
      </pre>
      <p>表达式支持：数字 / 字符串 / 布尔、+ - * / %、比较与逻辑运算、括号与内置函数。</p>

      <h3>3. 内置时间与随机函数</h3>
      <ul>
        <li><code>now()</code>：当前 Unix 时间戳（秒）</li>
        <li><code>time_hour()</code> / <code>time_minute()</code> / <code>time_second()</code>：当前时分秒</li>
        <li><code>rand()</code>：随机整数 (0-999999999)</li>
        <li><code>rand_int(min, max)</code>：给定区间内随机整数</li>
        <li><code>rand_bool()</code>：随机布尔值</li>
        <li><code>rand_choice(a, b, ...)</code>：从多个候选中随机选择一个</li>
      </ul>

      <h3>4. 说话与停顿</h3>
      <pre class="code">
speak Narrator "你好，VoxStage！"
sleep 1000  # 单位：毫秒
      </pre>
      <p>
        可以在角色名后添加针对本句的参数覆写，例如：
      </p>
      <pre class="code">
speak Narrator(language = "zh", speaker_id = "0") "你好，VoxStage！"
      </pre>

      <h3>5. BGM 相关语句</h3>
      <pre class="code">
bgm "bgm.mp3" loop
bgm_volume 0.5
bgm_pause
bgm_resume
bgm_stop
      </pre>
      <p>BGM 路径中也可以使用 <code>${变量}</code> 进行插值。</p>
    </section>

    <section class="help-section">
      <h2>更多文档</h2>
      <p>
        更详细的设计说明与示例，请参见仓库根目录的
        <code>README.md</code> 与 <code>docs/gui-phase5.md</code>。
      </p>
    </section>
  </div>
</template>

<style scoped>
.help-view {
  max-width: 960px;
  margin: 0 auto;
  padding: 0.5rem 0 1rem;
  font-size: 0.9rem;
  color: #111827;
}

.help-section + .help-section {
  margin-top: 1.5rem;
}

.help-section h2 {
  margin: 0 0 0.5rem;
  font-size: 1.1rem;
}

.help-section h3 {
  margin: 0.75rem 0 0.25rem;
  font-size: 0.95rem;
}

.help-section p {
  margin: 0.25rem 0;
}

.help-section ul,
.help-section ol {
  margin: 0.25rem 0 0.25rem 1.25rem;
  padding: 0;
}

.code {
  background: #111827;
  color: #e5e7eb;
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
    "Liberation Mono", "Courier New", monospace;
  font-size: 0.8rem;
  overflow-x: auto;
}

code {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
    "Liberation Mono", "Courier New", monospace;
  font-size: 0.8rem;
  background: #e5e7eb;
  padding: 0 0.15rem;
  border-radius: 0.15rem;
}
</style>

