import { Box, Text, SimpleGrid, Spinner, VStack, useColorModeValue, Image, Button, HStack } from "@chakra-ui/react";
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import epicGamesIcon from "@/assets/epic-games.webp";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface EpicGame {
  id: string;
  title: string;
  cover: string;
  original_price: number;
  original_price_desc: string;
  description: string;
  seller: string;
  is_free_now: boolean;
  free_start: string;
  free_start_at: number;
  free_end: string;
  free_end_at: number;
  link: string;
}

interface EpicResponse {
  code?: number;
  message: string;
  data: EpicGame[];
}

function GameCard({ game, fillHeight }: { game: EpicGame; fillHeight?: boolean }) {
  const { t } = useTranslation();

  const handleClaim = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(game.link);
    } catch (error) {
      window.open(game.link, "_blank");
    }
  };

  return (
    <Box
      position="relative"
      borderRadius="2xl"
      overflow="hidden"
      cursor="pointer"
      onClick={handleClaim}
      transition="box-shadow 0.3s ease"
      _hover={{
        boxShadow: "0 12px 30px rgba(0,0,0,0.4)",
      }}
      h={fillHeight ? "100%" : "480px"}
      minH={fillHeight ? "300px" : undefined}
    >
      <Image
        src={game.cover}
        alt={game.title}
        w="100%"
        h="100%"
        objectFit="cover"
        fallback={<Box w="100%" h="100%" bg="gray.800" />}
      />
      
      <Box
        position="absolute"
        top={0}
        left={0}
        right={0}
        bottom={0}
        bg="linear-gradient(to bottom, rgba(0,0,0,0.1) 0%, rgba(0,0,0,0.3) 50%, rgba(0,0,0,0.95) 100%)"
      />

      <Box position="absolute" top={3} right={3}>
        <Box
          bg="rgba(0,0,0,0.6)"
          backdropFilter="blur(10px)"
          px={3}
          py={1}
          borderRadius="lg"
          border="1px solid"
          borderColor="rgba(255,255,255,0.1)"
        >
          <Text fontSize="xs" color="white" fontWeight="medium" whiteSpace="nowrap">
            {t('epic.deadline', { date: game.free_end })}
          </Text>
        </Box>
      </Box>

      <Box position="absolute" bottom={0} left={0} right={0} p={5}>
        <VStack align="start" spacing={3}>
          <Text
            fontSize="xl"
            fontWeight="bold"
            color="white"
            lineHeight="shorter"
            noOfLines={2}
          >
            {game.title}
          </Text>
          
          <Text
            fontSize="sm"
            color="gray.300"
            noOfLines={2}
            lineHeight="tall"
          >
            {game.description}
          </Text>

          <Button
            bg="white"
            color="gray.900"
            _hover={{ bg: "gray.100" }}
            size="sm"
            borderRadius="xl"
            px={6}
            py={4}
            fontWeight="bold"
            fontSize="sm"
            width="100%"
            onClick={(e) => {
              e.stopPropagation();
              handleClaim();
            }}
          >
            {t('epic.claimNow')}
          </Button>
        </VStack>
      </Box>
    </Box>
  );
}

export default function EpicFreePage() {
  const { t } = useTranslation();
  const [games, setGames] = useState<EpicGame[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const bgColor = useColorModeValue("gray.50", "#0a0a0a");

  useEffect(() => {
    const fetchEpicGames = async () => {
      try {
        setLoading(true);
        setError(null);
        
        const response = await fetch("https://uapis.cn/api/v1/game/epic-free");
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}`);
        }
        const data: EpicResponse = await response.json();
        
        if (data.data && data.data.length > 0) {
          const freeGames = data.data.filter((game) => game.is_free_now === true);
          setGames(freeGames);
        } else {
          setError(data.message || t('epic.errorFetch'));
        }
      } catch (err) {
        console.error("Failed to fetch Epic games:", err);
        setError(t('epic.errorNetwork'));
      } finally {
        setLoading(false);
      }
    };

    fetchEpicGames();
  }, []);

  const shouldFillHeight = games.length >= 1 && games.length <= 3;

  return (
    <Box 
      h={shouldFillHeight ? "calc(100vh - 140px)" : "auto"} 
      display={shouldFillHeight ? "flex" : "block"}
      flexDirection={shouldFillHeight ? "column" : undefined}
      overflowY={shouldFillHeight ? "hidden" : "auto"}
    >
      <HStack mb={6} spacing={3} flexShrink={0}>
        <Image
          src={epicGamesIcon}
          alt="Epic Games"
          w="32px"
          h="32px"
          objectFit="contain"
        />
        <Text fontSize="3xl" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
          {t('epic.title')}
        </Text>
      </HStack>

      {loading ? (
        <VStack flex={1} justify="center" spacing={4} py={20}>
          <Spinner size="xl" thickness="3px" speed="0.65s" color="teal.500" />
          <Text color={subTextColor} fontSize="md">
            {t('epic.loading')}
          </Text>
        </VStack>
      ) : error ? (
        <VStack flex={shouldFillHeight ? 1 : undefined} justify="center" spacing={4} py={shouldFillHeight ? undefined : 20}>
          <Text color="red.500" fontSize="lg" fontWeight="medium">
            {error}
          </Text>
          <Button
            variant="outline"
            colorScheme="teal"
            onClick={() => window.location.reload()}
          >
            {t('epic.reload')}
          </Button>
        </VStack>
      ) : games.length === 0 ? (
        <VStack flex={shouldFillHeight ? 1 : undefined} justify="center" py={shouldFillHeight ? undefined : 20}>
          <Text color={subTextColor} fontSize="lg">
            {t('epic.noGames')}
          </Text>
        </VStack>
      ) : (
        <SimpleGrid
          flex={shouldFillHeight ? 1 : undefined}
          columns={games.length === 1 ? 1 : games.length === 2 ? 2 : games.length === 3 ? 3 : { base: 1, md: 2, lg: 3 }}
          spacing={6}
          overflow="hidden"
        >
          {games.map((game) => (
            <GameCard key={game.id} game={game} fillHeight={shouldFillHeight} />
          ))}
        </SimpleGrid>
      )}
    </Box>
  );
}
