import { memo } from "react";
import { Box, Image } from "@chakra-ui/react";
import { Music as MusicIcon } from "lucide-react";
import { hexToRgba } from "@/lib/color-utils";

interface VinylDiscProps {
  isPlaying: boolean;
  /** 胶片颜色（同时是歌词高亮色），hex */
  accentColor: string;
  /** 专辑封面 URL，空串时渲染占位圆 */
  coverUrl: string;
}

// 胶片压制纹路：细密均匀纹由盘体上的 repeating-radial-gradient 承担，
// 此层只叠少量宽弧带，经 SVG 湍流做低频小幅位移后成为平滑弯曲的明暗胶纹（仿 QQ 音乐）。
// 模块级生成一次。
const vinylGrooveTexture = `data:image/svg+xml,${encodeURIComponent(
  (() => {
    const bands: Array<[number, string, number, number]> = [
      [60, "#fff", 26, 0.10], [105, "#000", 18, 0.08], [150, "#fff", 30, 0.11],
      [205, "#000", 16, 0.08], [245, "#fff", 24, 0.10], [290, "#000", 20, 0.08],
      [330, "#fff", 22, 0.09],
    ];
    const circles = bands
      .map(([r, c, w, o]) => `<circle cx='350' cy='350' r='${r}' stroke='${c}' stroke-width='${w}' opacity='${o}'/>`)
      .join("");
    return (
      `<svg xmlns='http://www.w3.org/2000/svg' width='700' height='700'>` +
      `<defs><filter id='w' x='-20%' y='-20%' width='140%' height='140%'>` +
      `<feTurbulence type='fractalNoise' baseFrequency='0.005 0.018' numOctaves='2' seed='11' result='n'/>` +
      `<feDisplacementMap in='SourceGraphic' in2='n' scale='16' xChannelSelector='R' yChannelSelector='G'/>` +
      `</filter></defs><g fill='none' filter='url(#w)'>${circles}</g></svg>`
    );
  })(),
)}`;

/**
 * 透明彩胶样式：右上角伸出的半透明彩色胶片（黑胶唱片）
 * 纯 CSS 实现、无内部状态：旋转/光晕呼吸均由 CSS 动画驱动，播放暂停切换零重渲染成本。
 * 仿 QQ 音乐彩胶：盘体半透明透出白底、同心唱片细纹、conic 反光不随盘旋转、封面随盘转动。
 */
function VinylDiscInner({ isPlaying, accentColor, coverUrl }: VinylDiscProps) {
  const hasCover = !!coverUrl;
  return (
    <Box
      position="absolute"
      inset={0}
      overflow="hidden"
      pointerEvents="none"
      zIndex={1}
      aria-hidden
      sx={{
        "--disc": "min(80vh, 48vw)",
        "@keyframes vinylDiscIn": {
          from: { transform: "translate(6%, -6%) scale(0.94)", opacity: 0 },
          to: { transform: "translate(0%, 0%) scale(1)", opacity: 1 },
        },
        "@keyframes vinylSpin": {
          from: { transform: "rotate(0deg)" },
          to: { transform: "rotate(360deg)" },
        },
        "@keyframes vinylGlowBreath": {
          "0%, 100%": { transform: "scale(1)", opacity: 0.85 },
          "50%": { transform: "scale(1.06)", opacity: 1 },
        },
      }}
    >
      {/* 光晕层：与胶片同心的大范围柔光，播放时呼吸起伏 */}
      <Box
        position="absolute"
        width="calc(var(--disc) * 1.7)"
        height="calc(var(--disc) * 1.7)"
        top="calc(var(--disc) * -0.67)"
        right="calc(var(--disc) * -0.71)"
        borderRadius="full"
        background={`radial-gradient(circle, ${hexToRgba(accentColor, 0.30)} 0%, ${hexToRgba(accentColor, 0.13)} 42%, rgba(255,255,255,0) 68%)`}
        sx={{ animation: isPlaying ? "vinylGlowBreath 5s ease-in-out infinite" : undefined }}
      />
      {/* 胶片盘（入场动画层） */}
      <Box
        position="absolute"
        width="var(--disc)"
        height="var(--disc)"
        top="calc(var(--disc) * -0.32)"
        right="calc(var(--disc) * -0.36)"
        borderRadius="full"
        sx={{ animation: "vinylDiscIn 0.7s cubic-bezier(0.25, 0.8, 0.35, 1)" }}
      >
        {/* 旋转层：半透明盘体 + 唱片细纹 + 斑驳质感 + 封面 + 中孔 */}
        <Box
          position="absolute"
          inset={0}
          borderRadius="full"
          sx={{
            animation: "vinylSpin 24s linear infinite",
            animationPlayState: isPlaying ? "running" : "paused",
            background: `
              url("${vinylGrooveTexture}") center / 100% 100% no-repeat,
              repeating-radial-gradient(circle at 50% 50%, rgba(255,255,255,0.09) 0px, rgba(255,255,255,0.09) 1px, rgba(255,255,255,0) 1px, rgba(255,255,255,0) 6px),
              radial-gradient(circle at 34% 28%, rgba(255,255,255,0.14) 0%, rgba(255,255,255,0) 46%),
              radial-gradient(circle at 66% 72%, rgba(0,0,0,0.05) 0%, rgba(0,0,0,0) 52%),
              ${hexToRgba(accentColor, 0.56)}
            `,
            boxShadow: `inset 0 0 0 1px rgba(255,255,255,0.28), inset 0 0 60px ${hexToRgba(accentColor, 0.35)}`,
          }}
        >
          {/* 封面圆：随盘旋转（封面占比大、彩色外环窄） */}
          <Box
            position="absolute"
            width="70%"
            height="70%"
            top="15%"
            left="15%"
            borderRadius="full"
            overflow="hidden"
            boxShadow={`0 0 0 1px rgba(255,255,255,0.25), 0 6px 30px rgba(0,0,0,0.18)`}
          >
            {hasCover ? (
              <Image src={coverUrl} w="100%" h="100%" objectFit="cover" draggable={false} />
            ) : (
              <Box
                w="100%"
                h="100%"
                display="flex"
                alignItems="center"
                justifyContent="center"
                background={`linear-gradient(135deg, ${hexToRgba(accentColor, 0.85)}, ${hexToRgba(accentColor, 0.5)})`}
              >
                <MusicIcon size="26%" color="rgba(255,255,255,0.75)" strokeWidth={1.5} />
              </Box>
            )}
          </Box>
        </Box>
        {/* 高光层：conic 反光固定不随盘旋转（反光属于环境光） */}
        <Box
          position="absolute"
          inset={0}
          borderRadius="full"
          background="conic-gradient(from 215deg at 50% 50%, rgba(255,255,255,0) 0deg, rgba(255,255,255,0.15) 34deg, rgba(255,255,255,0.04) 72deg, rgba(255,255,255,0) 118deg, rgba(255,255,255,0) 242deg, rgba(255,255,255,0.10) 292deg, rgba(255,255,255,0) 338deg)"
        />
      </Box>
    </Box>
  );
}

export const VinylDisc = memo(VinylDiscInner);
