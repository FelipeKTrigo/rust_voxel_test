# Dungeon n Evolve

a ideia do projeto na fase 1 é ser um jogo perto do periodo medieval fantasioso com base em rust segue as caracteristicas tecnicas do mesmo.

- 3d
- deve ter uma tela inicial com botões[Iniciar Jogo(permite a criação de mundo ou seleção de mundos ja existentes),configurações(será descrito mais abaixo),sair]
- linguagem principal é o rust
- geração de mundo procedural
- geração de mundo usando a gpu como maquina de calculo principal
- geração usando noise texture para altura(avalie como as montanha conectaram para ficar continuo)
- geração usando noise texture para biomas combinado com a possibilidade do uso de voronoi(avalie se será util)
- estilo de voxel de geração de mundo em chuncking
- deve haver arvores que mesmo separadas por chuncks gerem corretamente
- organize a paleta de cores em uma classe para que fique organizado e facil de alterar mais tarde
- a geração de mundo deve ser feita em grande parte na gpu
- deve haver uma tela de pause apertando a tecla ESC, pausa todas simulações e interações do jogo
  - no menu deve ser possivel ter as opções[despausar(podendo ser tambem por apertar esc novamente), sair do jogo e salvar mundo(save em arquivo), alteração dos gráficos(chuncks visualizaveis, alteração de diminuição de particulas, alteração do lod, alteração de quantidade de threads do processador gerando o mundo e balanceamento com a gpu de geração do mundo)]
- o controlle inicial deve ser simplemente alteravel no menu de pausa, entre 3 modos[sobrevivencia(anda normal no chão pode pular 1 metro de altura), criativo(pode voar, caminhar, pular e quebrar os blocos a vontade), espectador(atravessa tudo voando, não pode andar, não interage ou quebra nada no mundo)]
- deve haver montanhas e ravinas
- devem ter 6 biomas principais
  - planices(flores, plantações, e um voxel mais plano, moinhos e casas simples de npcs)
  - florestas(arvores densas pinheiros, arbustos, e bagas vermelhas e azuis)
  - montanhas(ambiente rochoso, possui carvão, feldspato, magnetita(ferro), pirita(se misturado com agua vira acido sulfurico e dióxido de enxofre), bauxita e mais raramente ouro)
  - praias(areia, coqueiros(dão coco) e palmeiras, carangueijos e pescadores(com casas de pescador), algumas pequenas pedras)
  - deserto(dunas, cactos(raramente com frutas do cacto), muito raramente oasis)
  - oceanos(ondas constantes que se chocam com leveza nas praias e nas pedras, pequenos peixes(2d pixel art) que se movem em 3d, barcos de pescadores de rede, mar escuro e com pouca visibilidade abaixo da agua)
- deve se ter testes para comprovar as funcionalidades
