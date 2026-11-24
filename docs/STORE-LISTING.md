# Store listing (Microsoft Store)

Texts for Partner Center → the product → **Store listings**, one block per
language. Limits: short description 1,000 characters, description 10,000,
"What's new in this version" 1,500, up to 20 product features of 200
characters each, up to 7 search terms.

The screenshots for each language are in `store-art/screenshots/<language>/`
(1920 × 1080, in the order to upload them); the logos and the hero art are in
`store-art/` (see `tools/store_art.py`).

**Coin purchases in the Store** depend on the package: they are on only when
it was built with the repository variable `MSSTORE_PURCHASES` set to `on`
(and the server has the `SNOOK_LEGEND_MSSTORE_*` variables and the add-ons
are published; see [MSSTORE.md](MSSTORE.md), "Venda de moedas e VIP pela Store"). The
texts below don't mention buying anything: while the Store sale is off, use
them as they are, and the age rating questionnaire shouldn't mention
purchases with money either. Once it is on, add the block at the end of this
file to each description and tick "in-game purchases" in the questionnaire.

Each language also has the links for the listing's legal fields (the terms of
use and the privacy policy the server publishes, see [DEPLOY.pt-BR.md](DEPLOY.pt-BR.md),
section 10).

---

## Português (Brasil)

### Descrição curta

Sinuca de bola 8 em 3D, com física de verdade numa mesa de 8 pés, na regra brasileira ou na oficial. Jogue online valendo ranking, desafie quinze adversários com personalidade, encare o desafio diário e suba de nível. Grátis.

### Descrição

Snook Legend é um jogo de sinuca (bola 8) em 3D feito para quem gosta de jogar de verdade: cada tacada segue as leis da física, as bolas correm como as de verdade e as caçapas do meio são abertas, para a habilidade contar mais que a sorte.

Controle total do taco: mire, escolha a força, o ponto exato em que a ponta bate na branca e a inclinação do taco, que você levanta arrastando numa vista de lado da tacada. Sem números na tela: a força e a elevação você julga no olho, como numa mesa de verdade. Puxe, siga, dê efeito lateral, pule bolas e faça massé.

ONLINE
Entre no salão, converse, convide alguém ou use "Jogar agora" para encontrar um adversário do seu nível. Partidas valendo ranking ou amistosas, mesas abertas (públicas ou com senha), torneios de 4, 8 ou 16 jogadores com chave ao vivo, amigos, partidas diárias com até 3 dias por vez e o ranking mundial. Escolha uma de 64 fotos de perfil e o seu país.

CONTRA O COMPUTADOR
Quinze adversários com estilo e personalidade próprios, do Leo, que ainda está aprendendo, ao Viktor, o campeão, passando pelo Tito, que aposta tudo em cada tacada, e pelo Yuri, que nunca erra. Eles comentam a partida enquanto jogam (dá para desligar). Vença os quinze no Circuito e ganhe um taco exclusivo, dispute um torneio contra eles (mata-mata de dezesseis ou arena por pontos) ou desafie o "fantasma" de um amigo, que joga no estilo dele.

TODO DIA TEM ALGO NOVO
O desafio diário traz a mesma posição para todos, com ranking do dia. Os treinos com estrelas medem sua caçapa, seu efeito e suas tabelas. Cada partida dá experiência para o nível da sua conta.

SEU ESTILO
Tacos, panos, giz, jogos de bolas, mesas e salões temáticos, tudo conquistado com as moedas que você ganha jogando. Seus tacos ganham anéis e incrustações conforme você vence com eles.

FEITO PARA ENXERGAR BEM
Mesa iluminada por igual, bolas com números sempre legíveis e sombras suaves. Um placar que mostra as bolas de cada jogador na ordem em que caíram. Prefere jogar só de cima? Uma opção trava a câmera na vista de cima. Roda em praticamente qualquer PC, de notebooks antigos a máquinas gamer, com gráficos de Baixo a Ultra.

DUAS REGRAS
Jogue na regra brasileira, a dos bares e salões, com bolas de penalidade (falta passa a vez e elimina as menores bolas do adversário), ou na regra oficial da WPA, a dos torneios internacionais, com todas as tacadas cantadas e bola na mão em qualquer lugar depois de uma falta. Cada regra tem o seu rating e o seu ranking mundial, e você troca de regra quando quiser. As regras completas ficam a um clique, no menu e durante a partida.

Em 27 idiomas, entre eles português, inglês, espanhol, francês, alemão, italiano, russo, turco, árabe, hindi, chinês, japonês e coreano.

### Novidades desta versão

Versão 0.11.2: na regra oficial vale qualquer caçapa.
• Na regra oficial não se canta mais nada: uma bola do seu grupo em qualquer caçapa mantém a vez, e a 8 vale em qualquer caçapa.
• No fim de uma partida online, dá para esconder o resultado e ver a mesa e o chat.

Da versão 0.11.1: livre assim que a partida acaba, e um chat melhor.
• Quando a partida termina, os dois jogadores aparecem livres no salão na hora.
• O chat da partida fica mais alto ou mais baixo arrastando a borda de cima, e todos os chats têm uma barra de rolagem para arrastar.
• Muito mais caracteres chineses e japoneses no chat, e as escritas da Índia, do Sri Lanka, de Mianmar, do Camboja e da Etiópia.
• Numa janela pequena, abrir as frases rápidas não envia mais uma delas.

Da versão 0.11.0: medalhas de rubi, 26 reações novas e um perfil mais claro.

Da versão 0.9.0: o jogo em 27 idiomas, com as regras, as novidades e os nomes dos países traduzidos.

### Recursos do produto

1. Mesa de 8 pés com caçapas do meio abertas e física realista
2. Efeito, puxada, seguida, massé e bolas que pulam
3. Mira no olho: elevação numa vista de lado, sem números na tela
4. Partidas online valendo ranking ou amistosas, com ranking mundial
5. Torneios de 4, 8 ou 16 jogadores com chave ao vivo
6. Salão com chat, amigos, convites e mesas abertas ou com senha
7. Partidas diárias: até 3 dias para jogar a sua vez
8. Quinze adversários de computador com personalidade, que comentam a partida
9. O Circuito: vença os quinze e ganhe um taco exclusivo
10. Desafio diário com ranking do dia
11. 24 treinos com até três estrelas cada
12. Nível da conta, conquistas, estante de troféus e 64 fotos de perfil
13. Tacos, panos, mesas e salões temáticos conquistados jogando
14. Placar com as bolas de cada jogador na ordem em que caíram
15. Visual feito para enxergar bem, com opção de jogar só na vista de cima
16. Gráficos de Baixo a Ultra: roda em PCs antigos e brilha nos novos
17. Regra brasileira ou oficial (WPA), cada uma com seu rating e ranking, sempre à mão
18. Em 27 idiomas, do português ao chinês, árabe e hindi

### Termos de pesquisa

sinuca, bilhar, bola 8, snooker, pool, sinuca online, jogo de sinuca

### Termos e privacidade

- **Privacy policy URL:** https://snooklegend.app/pt/privacidade/
- **Additional license terms:** Termos de uso: https://snooklegend.app/pt/termos/
- **Support contact info:** o e-mail de atendimento (`LEGAL_CONTACT` no Vercel)

---

## English

### Short description

3D eight-ball pool with real physics on an 8-foot table, under the Brazilian or the official rules. Play rated games online, take on fifteen opponents with real personalities, beat the daily challenge and level up. Free.

### Description

Snook Legend is a 3D eight-ball pool game for people who like to really play: every shot follows the laws of physics, the balls roll like real ones and the side pockets are open, so skill counts for more than luck.

Full control of the cue: aim, choose the power, the exact point where the tip strikes the cue ball and the cue's elevation, which you raise by dragging it in a side view of the shot. No numbers on screen: you judge the power and the elevation by eye, as on a real table. Draw, follow, side spin, jump shots and massé.

ONLINE
Walk into the lounge, chat, invite someone or use "Play now" to find an opponent at your level. Rated or casual games, open tables (public or with a password), 4, 8 or 16-player tournaments with a live bracket, friends, daily games with up to 3 days per turn and the world ranking. Pick one of 64 profile pictures and your country.

AGAINST THE COMPUTER
Fifteen opponents with a style and personality of their own, from Leo, who is still learning, to Viktor, the champion, by way of Tito, who bets everything on every shot, and Yuri, who never misses. They talk about the game as they play (you can turn that off). Beat all fifteen on the Tour to earn an exclusive cue, play a tournament against them (a knockout of sixteen or an arena on points), or take on a friend's "ghost", which plays in their style.

SOMETHING NEW EVERY DAY
The daily challenge gives everyone the same position, with a ranking for the day. Drills with stars measure your potting, your spin and your banks. Every game earns experience for your account level.

YOUR STYLE
Cues, cloths, chalk, ball sets, themed tables and halls, all earned with the coins you win by playing. Your cues earn rings and inlays as you win with them.

MADE TO SEE CLEARLY
An evenly lit table, balls whose numbers always read and soft shadows. A score panel that shows each player's balls in the order they went down. Rather play from above? An option locks the camera to the top view. Runs on practically any PC, from old laptops to gaming rigs, with graphics from Low to Ultra.

TWO RULES
Play the Brazilian rule, the one of bars and pool halls, with penalty balls (a foul passes the turn and takes your opponent's lowest balls off the table), or the official WPA rules of international tournaments, with every shot called and ball in hand anywhere after a foul. Each has its own rating and world ranking, and you switch whenever you like. The full rules are a click away, in the menu and during a game.

In 27 languages, including English, Portuguese, Spanish, French, German, Italian, Russian, Turkish, Arabic, Hindi, Chinese, Japanese and Korean.

### What's new in this version

Version 0.11.2: under the official rules, any pocket counts.
• Under the official rules nothing is called: a ball of your group in any pocket keeps the table, and the 8 wins in any pocket.
• At the end of an online game, the result can be hidden to see the table and the chat.

From version 0.11.1: free as soon as the game ends, and a better chat.
• When a game ends, both players show as free in the lounge at once.
• The match chat gets taller or shorter by dragging its top edge, and every chat has a scroll bar to drag.
• Many more Chinese and Japanese characters in the chat, and the scripts of India, Sri Lanka, Myanmar, Cambodia and Ethiopia.
• In a small window, opening the quick phrases no longer sends one.

From version 0.11.0: ruby medals, 26 new reactions and a clearer profile.

From version 0.9.0: the game in 27 languages, with the rules, what's new and the countries' names translated.

### Product features

1. An 8-foot table with open side pockets and realistic physics
2. Spin, draw, follow, massé and jump shots
3. Aim by eye: elevation in a side view, no numbers on screen
4. Rated or casual online games, with a world ranking
5. 4, 8 or 16-player tournaments with a live bracket
6. A lounge with chat, friends, invitations and open or private tables
7. Daily games: up to 3 days to play your turn
8. Fifteen computer opponents with personality, who talk about the game
9. The Tour: beat all fifteen to earn an exclusive cue
10. A daily challenge with a ranking for the day
11. 24 drills with up to three stars each
12. Account level, achievements, a trophy shelf and 64 profile pictures
13. Cues, cloths, tables and themed halls earned by playing
14. A score panel with each player's balls in the order they went down
15. Made to see clearly, with an option to play from the top view only
16. Graphics from Low to Ultra: runs on old PCs and shines on new ones
17. The Brazilian or the official (WPA) rules, each with its own rating and ranking, always at hand
18. In 27 languages, from English to Chinese, Arabic and Hindi

### Search terms

pool, billiards, 8 ball, eight ball, snooker, pool online, pool game

### Terms and privacy

- **Privacy policy URL:** https://snooklegend.app/privacy/
- **Additional license terms:** Terms of use: https://snooklegend.app/terms/
- **Support contact info:** the customer service e-mail (`LEGAL_CONTACT` in Vercel)

---

## Español

### Descripción corta

Pool de bola 8 en 3D con física real en una mesa de 8 pies, con la regla brasileña o la oficial. Juega en línea con clasificación, enfréntate a quince rivales con personalidad, supera el desafío diario y sube de nivel. Gratis.

### Descripción

Snook Legend es un juego de pool (bola 8) en 3D hecho para quienes disfrutan jugar de verdad: cada tiro sigue las leyes de la física, las bolas ruedan como las de verdad y las troneras del medio son abiertas, para que la habilidad cuente más que la suerte.

Control total del taco: apunta, elige la fuerza, el punto exacto donde la punta golpea la blanca y la inclinación del taco, que levantas arrastrándolo en una vista de lado del tiro. Sin números en pantalla: la fuerza y la elevación las juzgas a ojo, como en una mesa de verdad. Retroceso, seguimiento, efecto lateral, saltos y massé.

EN LÍNEA
Entra en la sala, conversa, invita a alguien o usa "Jugar ahora" para encontrar un rival de tu nivel. Partidas con clasificación o amistosas, mesas abiertas (públicas o con contraseña), torneos de 4, 8 o 16 jugadores con cuadro en vivo, amigos, partidas diarias con hasta 3 días por turno y la clasificación mundial. Elige una de 64 fotos de perfil y tu país.

CONTRA LA COMPUTADORA
Quince rivales con estilo y personalidad propios, desde Leo, que todavía está aprendiendo, hasta Viktor, el campeón, pasando por Tito, que lo apuesta todo en cada tiro, y Yuri, que nunca falla. Comentan la partida mientras juegan (se puede desactivar). Vence a los quince en el Circuito y gana un taco exclusivo, juega un torneo contra ellos (eliminatoria de dieciséis o arena por puntos) o desafía al "fantasma" de un amigo, que juega con su estilo.

ALGO NUEVO CADA DÍA
El desafío diario trae la misma posición para todos, con clasificación del día. Los entrenamientos con estrellas miden tu puntería, tu efecto y tus bandas. Cada partida da experiencia para el nivel de tu cuenta.

TU ESTILO
Tacos, paños, tizas, juegos de bolas, mesas y salas temáticas, todo ganado con las monedas que obtienes jugando. Tus tacos ganan anillos e incrustaciones a medida que ganas con ellos.

HECHO PARA VER BIEN
Una mesa iluminada por igual, bolas con números siempre legibles y sombras suaves. Un marcador que muestra las bolas de cada jugador en el orden en que entraron. ¿Prefieres jugar desde arriba? Una opción fija la cámara en la vista superior. Funciona en prácticamente cualquier PC, desde portátiles antiguos hasta equipos gamer, con gráficos de Bajo a Ultra.

DOS REGLAS
Juega con la regla brasileña, la de los bares y salones, con bolas de penalización (la falta pasa el turno y elimina las bolas más bajas del rival), o con la regla oficial de la WPA, la de los torneos internacionales, con todos los tiros cantados y bola en mano en cualquier lugar tras una falta. Cada regla tiene su rating y su ranking mundial, y cambias de regla cuando quieras. Las reglas completas están a un clic, en el menú y durante la partida.

En 27 idiomas, entre ellos español, inglés, portugués, francés, alemán, italiano, ruso, turco, árabe, hindi, chino, japonés y coreano.

### Novedades de esta versión

Versión 0.11.2: en la regla oficial vale cualquier tronera.
• En la regla oficial ya no se canta nada: una bola de tu grupo en cualquier tronera mantiene el turno, y la 8 vale en cualquier tronera.
• Al final de una partida online, se puede ocultar el resultado para ver la mesa y el chat.

De la versión 0.11.1: libre en cuanto acaba la partida, y un chat mejor.
• Cuando la partida termina, los dos jugadores aparecen libres en el salón al instante.
• El chat de la partida se hace más alto o más bajo arrastrando su borde superior, y todos los chats tienen una barra de desplazamiento para arrastrar.
• Muchos más caracteres chinos y japoneses en el chat, y las escrituras de la India, Sri Lanka, Myanmar, Camboya y Etiopía.
• En una ventana pequeña, abrir las frases rápidas ya no envía una.

De la versión 0.11.0: medallas de rubí, 26 reacciones nuevas y un perfil más claro.

De la versión 0.9.0: el juego en 27 idiomas, con las reglas, las novedades y los nombres de los países traducidos.

### Características del producto

1. Mesa de 8 pies con troneras del medio abiertas y física realista
2. Efecto, retroceso, seguimiento, massé y saltos
3. Apunta a ojo: elevación en una vista de lado, sin números en pantalla
4. Partidas en línea con clasificación o amistosas, con clasificación mundial
5. Torneos de 4, 8 o 16 jugadores con cuadro en vivo
6. Sala con chat, amigos, invitaciones y mesas abiertas o con contraseña
7. Partidas diarias: hasta 3 días para jugar tu turno
8. Quince rivales de computadora con personalidad, que comentan la partida
9. El Circuito: vence a los quince y gana un taco exclusivo
10. Desafío diario con clasificación del día
11. 24 ejercicios con hasta tres estrellas cada uno
12. Nivel de cuenta, logros, vitrina de trofeos y 64 fotos de perfil
13. Tacos, paños, mesas y salas temáticas ganados jugando
14. Un marcador con las bolas de cada jugador en el orden en que entraron
15. Hecho para ver bien, con opción de jugar solo en la vista superior
16. Gráficos de Bajo a Ultra: funciona en PCs antiguos y brilla en los nuevos
17. Regla brasileña u oficial (WPA), cada una con su rating y ranking, siempre a mano
18. En 27 idiomas, del español al chino, el árabe y el hindi

### Términos de búsqueda

pool, billar, bola 8, snooker, pool en línea, juego de pool, billar online

### Términos y privacidad

- **Privacy policy URL:** https://snooklegend.app/es/privacidad/
- **Additional license terms:** Términos de uso: https://snooklegend.app/es/terminos/
- **Support contact info:** el correo de atención (`LEGAL_CONTACT` en Vercel)

---

## Later: when coin purchases are on

Add this paragraph to each description (before the last line), replace
**X** and **Y** with the prices of the cheapest and the dearest coin pack
and **Z** with the price of the month of VIP, as set in Partner Center
(Add-ons → Pricing; drop the sentence about VIP if `vip_month` is not on
sale), change "Grátis." / "Free." /
"Gratis." at the end of the short descriptions to "Grátis, com compras
opcionais de moedas." / "Free, with optional coin purchases." / "Gratis,
con compras opcionales de monedas.", and add a line to "What's new".

**Português:**
COMPRAS NO JOGO
O jogo é grátis. As moedas vêm de jogar (partidas valendo ranking e um bônus por dia jogado) e também podem ser compradas, de forma opcional, em pacotes de R$ X a R$ Y. Elas servem só para itens cosméticos: nada que faça ganhar partidas. O VIP (itens exclusivos e 400 moedas) é vendido um mês por vez, por R$ Z, e não se renova sozinho.

**English:**
IN-GAME PURCHASES
The game is free. Coins come from playing (rated games and a bonus for every day you play) and can also be bought, if you like, in packs from US$ X to US$ Y. They are only for cosmetic items: nothing that wins games. VIP (exclusive items and 400 coins) is sold a month at a time, for US$ Z, and doesn't renew by itself.

**Español:**
COMPRAS EN EL JUEGO
El juego es gratis. Las monedas se ganan jugando (partidas con clasificación y un bono por cada día que juegas) y también se pueden comprar, si quieres, en paquetes de US$ X a US$ Y. Sirven solo para objetos cosméticos: nada que haga ganar partidas. El VIP (artículos exclusivos y 400 monedas) se vende un mes a la vez, por US$ Z, y no se renueva solo.
