import {
  Box,
  HStack,
  VStack,
  Text,
  IconButton,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  useColorModeValue,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  Tooltip,
} from "@chakra-ui/react";
import { useState, useEffect } from "react";

import { Store } from "@tauri-apps/plugin-store";
import { useTranslation } from "react-i18next";
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Volume2,
  VolumeX,
  ListMusic,
  Repeat,
  Repeat1,
  Shuffle,
  Music,
} from "lucide-react";
import { LiquidGlassCard } from "./special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useMusicPlayer } from "@/contexts/music-context";


type PlayMode = "list" | "shuffle" | "one";

let storeInstance: Store | null = null;

const getStore = async (): Promise<Store> => {
  if (!storeInstance) {
    storeInstance = await Store.load("music-player-settings.json");
  }
  return storeInstance;
};

export function MusicPlayer() {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const {
    musicFiles,
    currentIndex,
    isPlaying,
    currentTime,
    duration,
    volume,
    isMuted,
    playMode,
    currentFileName,
    togglePlay,
    setVolume,
    toggleMute,
    nextTrack,
    prevTrack,
    seekTo,
    playTrack,
  } = useMusicPlayer();

  const [localPlayMode, setLocalPlayMode] = useState<PlayMode>("list");

  const bgColor = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const progressBg = useColorModeValue("gray.200", "#333333");

  useEffect(() => {
    loadLocalSettings();
  }, []);

  useEffect(() => {
    if (localPlayMode !== playMode) {
      setLocalPlayMode(playMode);
    }
  }, [playMode, localPlayMode]);

  const loadLocalSettings = async () => {
    try {
      const s = await getStore();
      const savedMode = await s.get<PlayMode>("playMode");
      if (savedMode) {
        setLocalPlayMode(savedMode);
      }
    } catch (error) {
      console.error("Failed to load music settings:", error);
    }
  };

  const saveSetting = async (key: string, value: unknown) => {
    try {
      const s = await getStore();
      await s.set(key, value);
      await s.save();
    } catch (error) {
      console.error("Failed to save setting:", error);
    }
  };

  const handleModeToggle = () => {
    const modes: PlayMode[] = ["list", "shuffle", "one"];
    const currentModeIndex = modes.indexOf(localPlayMode);
    const nextMode = modes[(currentModeIndex + 1) % modes.length];
    setLocalPlayMode(nextMode);
    saveSetting("playMode", nextMode);
  };

  const formatTime = (time: number): string => {
    if (isNaN(time)) return "0:00";
    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes}:${seconds.toString().padStart(2, "0")}`;
  };

  if (musicFiles.length === 0) {
    const EmptyContent = (
      <HStack spacing={4} align="center" py={2}>
        <Music size={20} color={subTextColor} />
        <Text color={subTextColor} fontSize="sm">
          {t("musicPlayer.empty")}
        </Text>
        <Text color="teal.400" fontSize="xs" fontWeight="medium">
          public/music/
        </Text>
      </HStack>
    );

    if (liquidGlassEnabled) {
      return (
        <LiquidGlassCard p={3} mt={4}>
          {EmptyContent}
        </LiquidGlassCard>
      );
    }

    return (
      <Box
        bg={bgColor}
        borderRadius="xl"
        p={3}
        mt={4}
        border="1px solid"
        borderColor={borderColor}
      >
        {EmptyContent}
      </Box>
    );
  }

  const ModeIcon = localPlayMode === "one" ? Repeat1 : localPlayMode === "shuffle" ? Shuffle : Repeat;

  const PlayerContent = (
    <VStack spacing={2} align="stretch" w="100%">
      <HStack spacing={4} align="center">
        <VStack spacing={0} align="start" flex={1} minW={0}>
          <Text
            color={textColor}
            fontWeight="medium"
            fontSize="sm"
            noOfLines={1}
          >
            {currentFileName || t("musicPlayer.noTrack")}
          </Text>
          <Text color={subTextColor} fontSize="xs">
            {formatTime(currentTime)} / {formatTime(duration)}
          </Text>
        </VStack>

        <HStack spacing={1}>
          <Tooltip label={t("musicPlayer.previous")}>
            <IconButton
              aria-label="Previous"
              icon={<SkipBack size={18} />}
              size="sm"
              variant="ghost"
              onClick={prevTrack}
            />
          </Tooltip>

          <Tooltip label={isPlaying ? t("musicPlayer.pause") : t("musicPlayer.play")}>
            <IconButton
              aria-label={isPlaying ? "Pause" : "Play"}
              icon={isPlaying ? <Pause size={20} /> : <Play size={20} />}
              size="sm"
              colorScheme="teal"
              onClick={togglePlay}
            />
          </Tooltip>

          <Tooltip label={t("musicPlayer.next")}>
            <IconButton
              aria-label="Next"
              icon={<SkipForward size={18} />}
              size="sm"
              variant="ghost"
              onClick={nextTrack}
            />
          </Tooltip>

          <Tooltip label={
            localPlayMode === "list"
              ? t("musicPlayer.modeList")
              : localPlayMode === "shuffle"
                ? t("musicPlayer.modeShuffle")
                : t("musicPlayer.modeOne")
          }>
            <IconButton
              aria-label="Play mode"
              icon={<ModeIcon size={16} />}
              size="sm"
              variant="ghost"
              colorScheme={localPlayMode !== "list" ? "teal" : "gray"}
              onClick={handleModeToggle}
            />
          </Tooltip>
        </HStack>

        <HStack spacing={2} w="120px">
          <Tooltip label={isMuted ? t("musicPlayer.unmute") : t("musicPlayer.mute")}>
            <IconButton
              aria-label={isMuted ? "Unmute" : "Mute"}
              icon={isMuted ? <VolumeX size={16} /> : <Volume2 size={16} />}
              size="sm"
              variant="ghost"
              onClick={toggleMute}
            />
          </Tooltip>
          <Slider
            value={isMuted ? 0 : volume}
            onChange={(v) => setVolume(v)}
            min={0}
            max={1}
            step={0.01}
            size="sm"
          >
            <SliderTrack bg={progressBg}>
              <SliderFilledTrack bg="teal.400" />
            </SliderTrack>
            <SliderThumb />
          </Slider>
        </HStack>

        <Menu>
          <Tooltip label={t("musicPlayer.playlist")}>
            <MenuButton
              as={IconButton}
              aria-label="Playlist"
              icon={<ListMusic size={18} />}
              size="sm"
              variant="ghost"
            />
          </Tooltip>
          <MenuList maxH="300px" overflowY="auto">
            {musicFiles.map((file, index) => (
              <MenuItem
                key={index}
                onClick={() => playTrack(index)}
                bg={index === currentIndex ? "teal.500" : undefined}
                color={index === currentIndex ? "white" : textColor}
                _hover={{ bg: index === currentIndex ? "teal.600" : "gray.100" }}
              >
                <Text noOfLines={1} fontSize="sm">
                  {index + 1}. {file.name}
                </Text>
              </MenuItem>
            ))}
          </MenuList>
        </Menu>
      </HStack>

      <Slider
        value={currentTime}
        onChange={(v) => seekTo(v)}
        min={0}
        max={duration || 100}
        step={0.1}
        size="sm"
      >
        <SliderTrack bg={progressBg} h="4px">
          <SliderFilledTrack bg="teal.400" />
        </SliderTrack>
        <SliderThumb w="10px" h="10px" />
      </Slider>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard p={3} mt={4}>
        {PlayerContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={bgColor}
      borderRadius="xl"
      p={3}
      mt={4}
      border="1px solid"
      borderColor={borderColor}
    >
      {PlayerContent}
    </Box>
  );
}
