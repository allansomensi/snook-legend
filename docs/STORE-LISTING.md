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
Jogue na regra brasileira, a dos bares e salões, com bolas de penalidade (falta passa a vez e elimina as menores bolas do adversário), ou na regra oficial da WPA, a dos torneios internacionais, em que as bolas do seu grupo valem em qualquer caçapa, a 8 é cantada (um clique na caçapa) e a falta dá bola na mão em qualquer lugar. Cada regra tem o seu rating e o seu ranking mundial, e você troca de regra quando quiser. As regras completas ficam a um clique, no menu e durante a partida.

Em 27 idiomas, entre eles português, inglês, espanhol, francês, alemão, italiano, russo, turco, árabe, hindi, chinês, japonês e coreano.

### Novidades desta versão

Versão 0.12.2: mensagens privadas, um chat mais tranquilo e conquistas mais difíceis.
• Mensagens privadas: escreva para qualquer jogador pela caixa de entrada ou pelo perfil dele. Em Configurações → Jogo você escolhe quem pode escrever para você; menores de 18 anos, só amigos.
• Um chat mais tranquilo: nas partidas online o chat mostra só o que os jogadores escrevem; o que acontece na mesa fica no histórico da partida, no alto. Quer tudo junto? Configurações → Jogo.
• Conquistas mais difíceis: as sequências de vitórias agora são de 5, 10, 15 e 30, e o XP de cada conquista acompanha a medalha.
• Análise para VIP: a análise das tacadas e as estatísticas avançadas viraram uma página só, para membros VIP.
• Telas mais fluidas: a loja e a coleção rolam para baixo, as janelas somem suavemente, o botão de voltar do mouse volta e o teclado chega a todos os botões.

Da versão 0.12.1: tudo mais fluido, partidas contra o computador que começam como as online e conquistas vistas de perto.

Da versão 0.12.0: duplas, contra o relógio, a 8 cantada na regra oficial, a coleção e a caixa de entrada.

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
Play the Brazilian rule, the one of bars and pool halls, with penalty balls (a foul passes the turn and takes your opponent's lowest balls off the table), or the official WPA rules of international tournaments, where your group's balls count in any pocket, the 8 is called (a click on the pocket) and a foul gives ball in hand anywhere. Each has its own rating and world ranking, and you switch whenever you like. The full rules are a click away, in the menu and during a game.

In 27 languages, including English, Portuguese, Spanish, French, German, Italian, Russian, Turkish, Arabic, Hindi, Chinese, Japanese and Korean.

### What's new in this version

Version 0.12.2: private messages, a quieter chat and harder achievements.
• Private messages: write to any player from the inbox or from their profile. Settings → Game decides who can write to you; under 18, friends only.
• A quieter chat: in online games the chat keeps to what the players say; what happens at the table is in the game's history, at the top. Want it all together? Settings → Game.
• Harder achievements: wins in a row now go 5, 10, 15 and 30, and each achievement's XP follows its medal.
• The analysis for VIP: the shot analysis and the advanced statistics are now one page, for VIP members.
• Smoother screens: the shop and the collection scroll down, windows fade out, the mouse's back button goes back and the keyboard reaches every button.

From version 0.12.1: smoother everywhere, games against the computer that open like online ones, and achievements up close.

From version 0.12.0: doubles, against the clock, the 8 called under the official rules, the collection and the inbox.

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
Juega con la regla brasileña, la de los bares y salones, con bolas de penalización (la falta pasa el turno y elimina las bolas más bajas del rival), o con la regla oficial de la WPA, la de los torneos internacionales, en la que las bolas de tu grupo valen en cualquier tronera, la 8 se canta (un clic en la tronera) y la falta da bola en mano en cualquier lugar. Cada regla tiene su rating y su ranking mundial, y cambias de regla cuando quieras. Las reglas completas están a un clic, en el menú y durante la partida.

En 27 idiomas, entre ellos español, inglés, portugués, francés, alemán, italiano, ruso, turco, árabe, hindi, chino, japonés y coreano.

### Novedades de esta versión

Versión 0.12.2: mensajes privados, un chat más tranquilo y logros más difíciles.
• Mensajes privados: escribe a cualquier jugador desde el buzón o desde su perfil. En Ajustes → Juego eliges quién puede escribirte; los menores de 18, solo amigos.
• Un chat más tranquilo: en las partidas online el chat muestra solo lo que escriben los jugadores; lo que pasa en la mesa está en el historial de la partida, arriba. ¿Lo quieres todo junto? Ajustes → Juego.
• Logros más difíciles: las rachas de victorias ahora son de 5, 10, 15 y 30, y la XP de cada logro sigue a su medalla.
• El análisis para VIP: el análisis de los tiros y las estadísticas avanzadas son ahora una sola página, para los miembros VIP.
• Pantallas más fluidas: la tienda y la colección se desplazan hacia abajo, las ventanas se desvanecen, el botón de retroceso del ratón vuelve atrás y el teclado llega a todos los botones.

De la versión 0.12.1: más fluido en todas partes, partidas contra el ordenador que empiezan como las online y los logros de cerca.

De la versión 0.12.0: dobles, contrarreloj, la 8 cantada en la regla oficial, la colección y el buzón.

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
