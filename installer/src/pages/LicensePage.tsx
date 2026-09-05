import { useEffect } from "react";
import { motion } from "framer-motion";
import { useTranslation } from "react-i18next";

export default function LicensePage({ onAgreed }: { onAgreed: (v: boolean) => void }) {
  const { t } = useTranslation();

  useEffect(() => {
    onAgreed(true);
  }, [onAgreed]);

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -20 }}
      transition={{ duration: 0.3 }}
      style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}
    >
      <h2 className="page-title">{t("license_title")}</h2>
      <p className="page-subtitle">{t("license_desc")}</p>

      <div className="license-scroll">
        <p style={{ fontSize: 16, fontWeight: 700, marginBottom: 6 }}>新境盒（NexBox）最终用户许可协议</p>
        <p style={{ color: "#94a3b8", marginBottom: 6 }}>新境盒最新版本 · 最后更新日期：2026年7月</p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <p style={{ fontWeight: 600, marginTop: 10 }}>重要提示</p>
        <p>请仔细阅读本协议的全部条款和条件。安装或使用本软件即表示您同意受本协议的约束。如果您不同意本协议的任何条款，请不要安装或使用本软件。</p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <h3 style={{ fontSize: 15, fontWeight: 700, margin: "16px 0 8px" }}>一、软件许可</h3>
        <p style={{ fontWeight: 600, marginTop: 8 }}>1.1 许可性质</p>
        <p>本软件采用 <strong>GNU 通用公共许可证第 3 版（GPL-3.0）</strong> 发布，是自由软件。您可以在遵守 GPL-3.0 协议条款的前提下，自由地使用、复制、分发和修改本软件。</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>1.2 您的权利</p>
        <p>根据 GPL-3.0 协议，您享有以下权利：</p>
        <p>- <strong>使用自由</strong>：您可以出于任何目的使用本软件，包括个人、教育、商业等用途</p>
        <p>- <strong>复制自由</strong>：您可以自由复制和分发本软件的副本</p>
        <p>- <strong>修改自由</strong>：您可以修改本软件的源代码，以适应您的需求</p>
        <p>- <strong>分发修改版的自由</strong>：您可以分发修改后的版本，但必须同样采用 GPL-3.0 协议</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>1.3 源代码获取</p>
        <p>本软件的完整源代码可在以下地址获取：</p>
        <p>- GitHub 仓库：<a href="#" style={{ color: "#3b82f6" }}>github.com/MuLiuSaMa/NexBox</a></p>
        <p>如果您通过二进制形式获得本软件，您有权根据 GPL-3.0 协议第 6 条的规定，以合理的成本获取完整的对应源代码。</p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <h3 style={{ fontSize: 15, fontWeight: 700, margin: "16px 0 8px" }}>二、用户义务</h3>
        <p style={{ fontWeight: 600, marginTop: 8 }}>2.1 版权声明</p>
        <p>您在分发本软件（无论是否经过修改）时，必须保留原始的版权声明和许可声明。</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>2.2 Copyleft 条款</p>
        <p>如果您修改了本软件并进行分发，您必须：</p>
        <p>- 明确标注您对软件所做的修改</p>
        <p>- 以相同的 GPL-3.0 协议发布您的修改版本</p>
        <p>- 提供修改后版本的完整源代码</p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <h3 style={{ fontSize: 15, fontWeight: 700, margin: "16px 0 8px" }}>三、免责声明</h3>
        <p style={{ fontWeight: 600, marginTop: 8 }}>3.1 无担保声明</p>
        <p>本软件按"<strong>原样</strong>"（AS IS）提供，不提供任何明示或暗示的担保，包括但不限于：</p>
        <p>- 对适销性的暗示担保</p>
        <p>- 对特定用途适用性的暗示担保</p>
        <p>- 对非侵权性的暗示担保</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>3.2 责任限制</p>
        <p>在法律允许的最大范围内，作者及贡献者不对因使用或无法使用本软件而导致的任何损害承担责任，包括但不限于：直接损害、间接损害、偶然损害、特殊损害、后果性损害、利润损失、数据丢失、业务中断。</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>3.3 使用风险</p>
        <p>使用本软件的全部风险由您自行承担。本软件涉及系统级操作（如内存清理、注册表修改、系统优化等），虽然开发者已尽最大努力确保软件的安全性和稳定性，但仍无法完全排除意外情况。建议您在使用系统优化功能前备份重要数据。</p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <h3 style={{ fontSize: 15, fontWeight: 700, margin: "16px 0 8px" }}>四、第三方组件</h3>
        <p>本软件包含以下第三方开源组件，各组件受其各自许可证条款的约束：</p>
        <table style={{ width: "100%", borderCollapse: "collapse", margin: "8px 0", fontSize: 12 }}>
          <thead>
            <tr style={{ background: "#f8fafc", borderBottom: "1px solid #e2e8f0" }}>
              <th style={{ padding: "6px 8px", textAlign: "left", fontWeight: 600 }}>组件名称</th>
              <th style={{ padding: "6px 8px", textAlign: "left", fontWeight: 600 }}>许可证</th>
              <th style={{ padding: "6px 8px", textAlign: "left", fontWeight: 600 }}>说明</th>
            </tr>
          </thead>
          <tbody>
            <tr style={{ borderBottom: "1px solid #f1f5f9" }}><td style={{ padding: "5px 8px" }}>Tauri</td><td style={{ padding: "5px 8px" }}>MIT / Apache-2.0</td><td style={{ padding: "5px 8px" }}>桌面应用框架</td></tr>
            <tr style={{ borderBottom: "1px solid #f1f5f9" }}><td style={{ padding: "5px 8px" }}>LibreHardwareMonitorLib</td><td style={{ padding: "5px 8px" }}>MPL-2.0</td><td style={{ padding: "5px 8px" }}>硬件监控库</td></tr>
            <tr style={{ borderBottom: "1px solid #f1f5f9" }}><td style={{ padding: "5px 8px" }}>WinDivert</td><td style={{ padding: "5px 8px" }}>LGPL-3.0</td><td style={{ padding: "5px 8px" }}>网络数据包捕获</td></tr>
            <tr style={{ borderBottom: "1px solid #f1f5f9" }}><td style={{ padding: "5px 8px" }}>Wintun</td><td style={{ padding: "5px 8px" }}>GPL-2.0 / 商业</td><td style={{ padding: "5px 8px" }}>虚拟网络适配器</td></tr>
            <tr style={{ borderBottom: "1px solid #f1f5f9" }}><td style={{ padding: "5px 8px" }}>NVIDIA NVAPI</td><td style={{ padding: "5px 8px" }}>NVIDIA 专有许可</td><td style={{ padding: "5px 8px" }}>NVIDIA GPU API</td></tr>
            <tr style={{ borderBottom: "1px solid #f1f5f9" }}><td style={{ padding: "5px 8px" }}>React</td><td style={{ padding: "5px 8px" }}>MIT</td><td style={{ padding: "5px 8px" }}>前端框架</td></tr>
            <tr style={{ borderBottom: "1px solid #f1f5f9" }}><td style={{ padding: "5px 8px" }}>Rust 标准库</td><td style={{ padding: "5px 8px" }}>MIT / Apache-2.0</td><td style={{ padding: "5px 8px" }}>Rust 编程语言</td></tr>
          </tbody>
        </table>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <h3 style={{ fontSize: 15, fontWeight: 700, margin: "16px 0 8px" }}>五、隐私声明</h3>
        <p style={{ fontWeight: 600, marginTop: 8 }}>5.1 数据收集</p>
        <p>本软件核心功能<strong>纯本地运行</strong>，不会主动收集或上传您的个人数据。以下功能可能涉及网络连接：</p>
        <p>- <strong>自动更新检查</strong>：仅检查版本信息，不上传个人数据</p>
        <p>- <strong>公告获取</strong>：从服务器获取软件公告信息</p>
        <p>- <strong>改枪码平台</strong>：用户提交的内容将上传至服务器</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>5.2 数据安全</p>
        <p>我们重视您的隐私。本软件不会记录您的浏览历史、文件内容或其他个人敏感信息。</p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <h3 style={{ fontSize: 15, fontWeight: 700, margin: "16px 0 8px" }}>六、协议终止</h3>
        <p style={{ fontWeight: 600, marginTop: 8 }}>6.1 自动终止</p>
        <p>如果您违反本协议的任何条款，您的许可将自动终止。</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>6.2 终止后的义务</p>
        <p>协议终止后，您必须停止使用本软件，并销毁您所持有的本软件的全部副本（包括安装文件、备份等）。</p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <h3 style={{ fontSize: 15, fontWeight: 700, margin: "16px 0 8px" }}>七、其他条款</h3>
        <p style={{ fontWeight: 600, marginTop: 8 }}>7.1 协议修订</p>
        <p>作者保留随时修订本协议的权利。修订后的协议将在软件更新时生效。</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>7.2 可分割性</p>
        <p>如果本协议的任何条款被有管辖权的法院认定为无效或不可执行，该条款的无效不影响本协议其他条款的效力。</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>7.3 完整协议</p>
        <p>本协议构成您与作者之间关于使用本软件的完整协议，并取代所有先前的口头或书面协议。</p>

        <p style={{ fontWeight: 600, marginTop: 8 }}>7.4 联系信息</p>
        <p>如有任何问题或建议，可通过以下方式联系：</p>
        <p>- GitHub：<a href="#" style={{ color: "#3b82f6" }}>github.com/MuLiuSaMa/NexBox</a></p>
        <p>- 官方网站：<a href="#" style={{ color: "#3b82f6" }}>www.nexbox.top</a></p>
        <hr style={{ border: "none", borderTop: "1px solid #e2e8f0", margin: "12px 0" }} />

        <p style={{ fontWeight: 600 }}>继续安装即表示您已阅读并同意本协议的全部条款。</p>
      </div>
    </motion.div>
  );
}
