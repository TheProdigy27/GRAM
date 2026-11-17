import type { InfoEntry } from './ui/info-tip/info-tip';

/**
 * Explications des termes techniques, affichées par les « i » (`g-info`) des filtres, du
 * classement et de la fiche. Une à trois phrases, pour quelqu'un qui découvre les modèles locaux.
 */
export const GLOSSARY = {
  context: {
    term: 'Contexte',
    text: 'Longueur de conversation que le modèle garde en tête : vos messages, ses réponses et les fichiers joints. Elle se compte en tokens, des morceaux de mots : 8k, c’est environ 8 000 tokens, une dizaine de pages. Plus le contexte est long, plus il prend de mémoire, en plus du modèle.',
  },
  runtime: {
    term: 'Runtime',
    text: 'Logiciel qui charge le modèle et le fait tourner sur votre machine : Ollama, LM Studio ou llama.cpp. GRAM s’en sert pour les commandes de lancement et les conseils ; les estimations de vitesse et de mémoire ne changent pas.',
  },
  maxSize: {
    term: 'Taille max',
    text: 'Taille maximale des fichiers de la quantification, c’est-à-dire ce qu’il faut télécharger. En mémoire, il faut un peu plus : le contexte et le surcoût du runtime s’y ajoutent.',
  },
  quant: {
    term: 'Quantification',
    text: 'Version compressée des poids du modèle. Le chiffre donne à peu près les bits par poids : Q4_K_M (4,8 bits environ) est presque deux fois plus léger et plus rapide que Q8_0 (8,5 bits), pour une précision un peu moindre. F16 et BF16 ne sont pas compressés.',
  },
  bpw: {
    term: 'Bits par poids',
    text: 'Bits par poids réels, calculés sur les fichiers : la compression effective de la quantification, au-delà de son nom. Plus il y en a, plus le modèle est fidèle et lourd.',
  },
  footprint: {
    term: 'Taille',
    text: 'Taille des fichiers de la quantification. La barre place la mémoire nécessaire, contexte compris, sur l’échelle de la mémoire utilisable, la même pour toutes les lignes : en bleu la part en VRAM, en orange la part en RAM, en rouge un modèle trop gros.',
  },
  mode: {
    term: 'Mode',
    text: 'Où tourne le modèle. GPU : entièrement dans la VRAM de la carte graphique, le plus rapide. Hybride : une partie des couches déborde en RAM, ce qui ralentit tout (30/40 : 30 couches sur 40 en VRAM). CPU : calculé par le processeur, en RAM, le plus lent.',
  },
  tokS: {
    term: 'tok/s',
    text: 'Vitesse de génération estimée, en tokens par seconde. Dès 15 tok/s, la réponse s’affiche plus vite qu’on ne la lit ; dès 30, c’est très fluide. Le ~ rappelle que c’est une estimation (±30 %).',
  },
  ctxMax: {
    term: 'Ctx max',
    text: 'Plus long contexte qui tient dans la mémoire utilisable avec cette quantification, sans dépasser le contexte d’entraînement du modèle.',
  },
  score: {
    term: 'Score',
    text: 'Note sur 100 qui combine qualité, vitesse, marge mémoire et contexte, pondérés selon l’usage choisi, plus un peu de popularité. Survoler une note pour son détail ; les poids se règlent dans les paramètres.',
  },
  params: {
    term: 'Paramètres',
    text: 'Nombre de poids du réseau, en milliards (Md). Plus il y en a, plus le modèle en sait et raisonne bien, mais plus il est lourd et lent. Pour un MoE, seuls les paramètres actifs travaillent à chaque token.',
  },
  ctxTrain: {
    term: 'Contexte d’entraînement',
    text: 'Contexte le plus long prévu par les auteurs du modèle. Au-delà, il perd le fil : GRAM ne propose jamais plus, même si la mémoire le permet.',
  },
  arch: {
    term: 'Architecture',
    text: 'Famille de réseau déclarée dans le fichier GGUF (qwen3, llama, gemma3…) ; le runtime doit la connaître pour charger le modèle. Un MoE (mélange d’experts) n’active que quelques experts à chaque token : bien plus rapide qu’un modèle dense de même taille, il prend autant de mémoire.',
  },
  kv: {
    term: 'Cache KV',
    text: 'Mémoire où le modèle range le contexte de la conversation, token par token. Elle grandit avec le contexte ; un cache q8_0 ou q4_0 (dans les paramètres) la divise par deux ou par quatre.',
  },
  vision: {
    term: 'Projecteur vision',
    text: 'Fichier à part (mmproj) qui permet au modèle de lire des images. Il se charge en plus des poids.',
  },
  overhead: {
    term: 'Surcoût',
    text: 'Mémoire prise par le runtime lui-même : contexte CUDA, Vulkan ou Metal et buffers de calcul. Réglable dans les paramètres.',
  },
  calibration: {
    term: 'Calibration',
    text: 'GRAM compare ses estimations à vos mesures et en déduit l’efficacité réelle de votre GPU et de votre RAM. Recalibrer reporte ces valeurs dans les paramètres : les estimations suivantes collent mieux à votre machine.',
  },
} as const satisfies Record<string, InfoEntry>;
