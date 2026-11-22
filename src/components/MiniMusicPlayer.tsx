import { Box, IconButton, Tooltip, useColorModeValue } from "@chakra-ui/react";
import { useMusicPlayer } from "@/contexts/music-context";
import { StopCircle } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";

export function MiniMusicPlayer() {
  const { isPlaying, currentFileName, stopPlayback } = useMusicPlayer();

  const bgColor = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");

  if (!isPlaying) return null;

  return (
    <AnimatePresence>
      <motion.div
        initial={{ scale: 0.8, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        exit={{ scale: 0.8, opacity: 0 }}
        transition={{ duration: 0.2 }}
        style={{
          position: "fixed",
          bottom: "24px",
          left: "24px",
          zIndex: 100,
        }}
      >
        <Tooltip label={currentFileName || "正在播放"}>
          <Box
            bg={bgColor}
            borderRadius="full"
            data-corner="round"
            border="1px solid"
            borderColor={borderColor}
            boxShadow="md"
            p={1}
            cursor="pointer"
            _hover={{ boxShadow: "lg" }}
            sx={{
              transition: "all 0.15s",
              animation: "spin-slow 8s linear infinite",
            }}
          >
            <IconButton
              aria-label="Stop music"
              icon={<StopCircle size={20} color="#98DDD0" />}
              variant="ghost"
              borderRadius="full"
              data-corner="round"
              size="sm"
              onClick={(e) => {
                e.stopPropagation();
                stopPlayback();
              }}
            />
          </Box>
        </Tooltip>
      </motion.div>
    </AnimatePresence>
  );
}
