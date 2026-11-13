# part 2 do desenvolvimento Dungeon n Evolve

essa parte do desenvolvimento serve para polir os graficos e funcionalidades, vamos focar em coisas não tratadas anteriormente.

- as texturas semi-transparentes permitem ver através das chunks, quero que isso não seja possivel
- o mar usa a mesma textura de circulos, deve ser randomica a textura e quadrada se assemelhando a um mar
- o mar deve ter menos barcos
- o jogo deve ter ciclo de dias e semanas lunares
- o jogo deve ter ciclo de dia e noite
- deve haver um sol
- deve haver nuvens
- deve haver por do sol assim como nascer do sol
- a noite devem aparecer estrelas(pixeis com variação de tamanho e luminosidade no céu) e dependendo da fase da lua a propria.
- a noite raramente deve passar cometas pixeis em movimento com traço(aparecem e somem rapidamente)
- ao apertar ctrl o jogador deverá correr, e se o mesmo estiver voando(criativo ou espectador) deve se aumentar a velocidade de voo
- todos blocos agora devem ter 16x16 pixeis
- re estilize todos blocos para ter uma textura combinando com o que o bloco deve ser
- o perfil survival de ter a habilidade de quebrar blocos
- os blocos quebrados devem ficar caidos no chão girando e flutuando, eles devem ter uma escala bem menor do que o tamanho real deles
- quero que crie um ius para o survival e criativo
  - iu inventario do personagem
    - essa iu serve para pegar os blocos quebrados
    - essa iu terá 2 partes os blocos e a visão de personagem, assim como sua armadura, aneis, colares, luvas, bolsa, cinto(libera uma quantidade configuravel de slots para bugigangas(itens de melhoria do usuario exemplo lanterna, amuleto etc))
    - esse iu de inventario deve poder mover itens entre o a hotbar do usuario e o inventario
  - iu hotbar
    - essa iu deve ficar centralizada na tela do personagem na parte inferior
    - devem ser por padrão 5 slots
    - intes pegos do chão devem ir primeiro para a hot bar se não houver do mesmo no inventario, se houver os mesmos devem ir para o inventario
- quero que arrume a iu de grafico, preciso que a reestilize para que fique centralizada e em uma configuração de tela separada, pois hoje a mesma não combina com o resto das interfaces
