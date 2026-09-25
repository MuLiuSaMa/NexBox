/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** 商店版（MSIX）构建标志，见 src/lib/build-flags.ts */
  readonly VITE_STORE_BUILD?: string;
}

declare module "@/assets/*.png" {
  const value: string;
  export default value;
}

declare module "*.png" {
  const value: string;
  export default value;
}

declare module "*.svg" {
  const value: string;
  export default value;
}

declare module "lunar-javascript" {
  export class Solar {
    static fromYmd(year: number, month: number, day: number): Solar;
    getLunar(): Lunar;
  }
  export class Lunar {
    getFestivals(): string[];
  }
}
