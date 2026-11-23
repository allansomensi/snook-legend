# Microsoft Store

O jogo vai para a Microsoft Store como um pacote **MSIX**, feito do mesmo
código das outras versões. A Store assina o pacote, instala e atualiza
sozinha. O instalador `.msi` e os arquivos do GitHub continuam como estão.

## O que muda na versão da Store

O build da Store usa a feature `store` do jogo (`crates/client/src/channel.rs`):

- **Moedas e VIP comprados pela própria Store, nunca pelo Stripe.** A
  política da Store (10.8.1) pede que jogos vendam itens digitais pelo
  sistema de compras da Store. Os pacotes de moedas e **um mês de VIP**
  viram *add-ons* da Store (veja
  [Venda de moedas e VIP pela Store](#venda-de-moedas-e-vip-pela-store));
  enquanto o servidor não estiver configurado para isso, a aba Moedas explica
  que as moedas vêm de jogar e a aba VIP diz que o VIP não é vendido nesta
  versão, como antes. Na Store, o VIP é **um mês por compra, sem renovação
  automática**: quem quer mais um mês compra de novo, e o mês novo começa
  quando o atual acaba. O pacote de boas-vindas e o VIP de presente não são
  vendidos nesta versão. Moedas e itens comprados em outra versão continuam
  valendo (a política permite usar o que já foi comprado). Quem é VIP por
  uma assinatura de outra versão continua vendo até quando, sem o botão de
  gerenciar a assinatura.
- **Runtime do C embutido** (`+crt-static`): o `.exe` não depende do Visual
  C++ Redistributable.
- **Firewall da rede local já liberado** pelo pacote (portas 47879/TCP e
  47878/UDP, só em redes privadas e de domínio): hospedar uma partida na rede
  local não mostra o aviso do Windows.

| Arquivo | O que é |
|---|---|
| `crates/client/msix/AppxManifest.xml` | o manifesto do pacote (identidade, ícones, firewall) |
| `crates/client/msix/Assets/` | os ícones do pacote (`cargo test --release -p snook-legend export_msix_assets -- --ignored` os refaz a partir do emblema) |
| `tools/msix/make-msix.ps1` | preenche o manifesto e monta o `.msix` |
| `.github/workflows/msstore.yml` | gera o MSIX (o envio à Store é manual) |
| repositório `snooklegend.app`, `src/content/legal/` | os termos de uso e a política de privacidade, no site |

## Uma vez só: preparar a Store

### 1. Reservar o nome e anotar a identidade

No [Partner Center](https://partner.microsoft.com/dashboard): **Apps and
games → New product → MSIX or PWA app**, reserve o nome. Depois, em
**Product management → Product identity**, anote os valores e crie as
variáveis do repositório (GitHub: **Settings → Secrets and variables →
Actions → Variables**):

| Variável | Valor do Partner Center |
|---|---|
| `MSSTORE_IDENTITY_NAME` | `Package/Identity/Name` |
| `MSSTORE_PUBLISHER` | `Package/Identity/Publisher` (começa com `CN=`) |
| `MSSTORE_PUBLISHER_DISPLAY_NAME` | `Package/Properties/PublisherDisplayName` |
| `MSSTORE_DISPLAY_NAME` | o nome reservado |

### 2. Termos de uso e política de privacidade

Ficam no site do jogo (repositório `snooklegend.app`), em três línguas:

| Página | Português | English | Español |
|---|---|---|---|
| Termos de uso | `https://snooklegend.app/pt/termos/` | `/terms/` | `/es/terminos/` |
| Política de privacidade | `https://snooklegend.app/pt/privacidade/` | `/privacy/` | `/es/privacidad/` |

Antes de usá-los, **leia os textos** (`src/content/legal/` no repositório do
site) e ajuste o que for preciso: eles descrevem o jogo como ele é hoje, mas
não são uma revisão jurídica. Quem oferece o jogo (nome, CPF ou CNPJ,
endereço, e-mail) vem das variáveis `LEGAL_*` do projeto no Vercel (veja o
README do site e a seção 10 de [DEPLOY.pt-BR.md](DEPLOY.pt-BR.md)).

No Partner Center (**Store listings**, em cada idioma, e **Properties**):

| Campo | Valor |
|---|---|
| **Privacy policy URL** | `https://snooklegend.app/privacy/` (na listagem em português: `/pt/privacidade/`; em espanhol: `/es/privacidad/`) |
| **Additional license terms** | "Termos de uso: https://snooklegend.app/pt/termos/" (em inglês: `/terms/`; em espanhol: `/es/terminos/`) |
| **Support contact info** | o mesmo e-mail de `LEGAL_CONTACT` |
| **Website** | `https://snooklegend.app` |

### 3. Servidor sempre acordado

O servidor oficial roda no plano **Starter** da Render, que não dorme. No
plano gratuito o servidor dorme depois de 15 minutos sem tráfego e o primeiro
jogador espera cerca de um minuto: não use o plano gratuito para o servidor
dos jogadores.

### 4. Primeiro envio (pelo site)

A primeira submissão precisa ser feita no Partner Center, porque ela inclui a
página da loja, a classificação etária e o preço:

1. Gere o pacote: **Actions → Microsoft Store → Run workflow**. Com as
   variáveis do passo 1 o pacote já sai com a identidade certa. Baixe o
   artefato `msix` da execução.
2. **Start your submission** e preencha:
   - **Pricing and availability:** grátis; mercados.
   - **Properties:** categoria *Games*; marque que o jogo tem chat e jogo
     online; requisitos: placa de vídeo com OpenGL.
   - **Age ratings:** responda o questionário (IARC). Diga que há
     **interação entre usuários (chat)**, **compartilhamento de nome de
     usuário** e **itens compráveis com moeda do jogo** (e, com a venda de
     moedas ligada, **compras com dinheiro**: veja
     [Venda de moedas e VIP pela Store](#venda-de-moedas-e-vip-pela-store)). Não há
     *loot boxes*. O chat começa desligado para todos: quem tem 18 anos ou
     mais o liga ao informar a idade, na primeira vez que escreve, e nas
     contas de menores de 18 anos só o responsável o liga (art. 20 da Lei
     15.211/2025, o ECA Digital; veja a seção 4 dos termos de uso).
   - **Packages:** envie o `.msix` baixado.
   - **Store listings** (português, inglês e espanhol): os textos de
     [STORE-LISTING.md](STORE-LISTING.md), as capturas de tela de
     `store-art/screenshots/<idioma>/` (1920×1080, na ordem dos nomes) e o
     link de suporte.
3. Envie para certificação (até três dias úteis).

## Venda de moedas e VIP pela Store

### Ligar e desligar as vendas (variável `MSSTORE_PURCHASES`)

As vendas pela Store dependem **do pacote**, não do servidor: o jogo da
Store só oferece pacotes de moedas e o mês de VIP quando é compilado com a
*feature* `store-purchases`. O workflow decide isso pela variável do
repositório **`MSSTORE_PURCHASES`** (Settings → Secrets and variables →
Actions → **Variables**):

| `MSSTORE_PURCHASES` | O pacote da Store |
|---|---|
| `on` | vende os pacotes de moedas e o mês de VIP (add-ons) |
| qualquer outro valor, ou sem a variável | **não vende nada por dinheiro**: as moedas vêm só de jogar, a aba VIP diz que ele não é vendido nesta versão |

As variáveis `SNOOK_LEGEND_MSSTORE_*` do Render **podem ficar como estão**:
o servidor continua pronto, só o jogo não mostra as ofertas. Uma compra
feita numa versão anterior e que nunca chegou ainda é entregue.

**Para tirar as compras e baixar a classificação etária** (o IARC dá 14+
no Brasil com compras no jogo):

1. Apagar a variável `MSSTORE_PURCHASES` (ou mudar para `off`).
2. Gerar o MSIX (**Actions → Microsoft Store → Run workflow**, ou a tag da
   versão) e conferir no log do passo de compilação
   `Store build with: store`.
3. Partner Center: em cada add-on, **Pricing and availability →
   Visibility: Stop acquisition** (ou deixe de vendê-los), para nada
   aparecer à venda na página da Store.
4. Partner Center → **Age ratings**: refazer o questionário IARC dizendo
   que **não há compras com dinheiro** no jogo (os itens continuam
   compráveis só com a moeda ganha jogando).
5. Enviar a submissão com o pacote novo.

Para voltar a vender: `MSSTORE_PURCHASES` = `on`, reabrir os add-ons e
refazer o questionário (com compras).

### Como a venda funciona

Os quatro pacotes de moedas (`coins_500`, `coins_1200`, `coins_2800`,
`coins_7000`) e o mês de VIP (`vip_month`) são vendidos como *add-ons* da
Store. O dinheiro fica com a Microsoft (que repassa pelo Partner Center,
menos a comissão dela); o jogo nunca vê dados de pagamento.

**Roteiro, na ordem** (detalhes de cada passo abaixo):

1. Partner Center: criar os 5 *add-ons* como *developer-managed
   consumable* e enviá-los para certificação; anotar o Store ID de cada um.
2. Microsoft Entra: registrar uma aplicação para o servidor, criar um
   *client secret* e pôr o *Application (client) ID* em **Services →
   Product collections and purchases** do jogo.
3. Render: definir as quatro variáveis `SNOOK_LEGEND_MSSTORE_*` e publicar o
   servidor novo (as migrações 0013, 0014 e 0015 rodam sozinhas); conferir no
   log `Microsoft Store purchases on (4 packs, VIP month)`.
4. Atualizar o questionário IARC (compras no jogo) e a descrição da loja
   (faixa de preços).
5. Gerar o MSIX da versão nova, publicá-lo para um público privado e testar
   uma compra de verdade; depois liberar para todos.

### Como funciona (e por que é seguro)

1. O jogador clica num pacote (ou no mês de VIP); o jogo abre a **janela de
   compra da própria Store**. O jogo nunca decide sozinho que algo foi
   comprado.
2. Depois da compra, o servidor dá ao jogo um *ticket* (um token da sua
   aplicação no Microsoft Entra que só serve para a Store criar uma chave)
   e um identificador anônimo do jogador. A Store, no computador do jogador,
   cria com isso uma **chave da Microsoft Store** e o jogo a manda ao servidor.
3. O servidor confere que a chave foi feita para aquele jogador, **pergunta à
   Microsoft** quais pacotes aquela conta Microsoft comprou e ainda não
   recebeu e, para cada um, grava um pedido com um `trackingId` próprio
   **antes** de pedir à Microsoft que o consuma. As moedas (e o mês de VIP)
   só entram depois que a Microsoft confirma, na mesma transação do banco que
   fecha o pedido.
4. Se a resposta da Microsoft se perder (rede, servidor reiniciando), o
   pedido fica pendente e é reenviado com o **mesmo** `trackingId` (a
   Microsoft responde sempre igual para o mesmo `trackingId`): na próxima vez
   que o jogador abrir a loja e, sozinho, a cada minuto. Um servidor
   "reserva" o pedido no banco enquanto o envia, então nem dois servidores
   ao mesmo tempo (durante um deploy) o mandam juntos. Uma recusa só é aceita
   como "nada foi consumido" no primeiro envio; num reenvio ela não prova
   nada, e o pedido continua sendo reenviado. Nada é creditado duas vezes, e
   nada pago se perde. Se em uma semana não houver resposta, o pedido fica
   marcado como *stuck* no painel para a equipe olhar.

Pedidos e créditos aparecem no painel da equipe, na página do jogador
(**Microsoft Store purchases** e **Payments**); a receita da Store aparece
só como contagem em **Metrics**, porque o valor fica no Partner Center.
Num pedido *stuck*, o botão **Send again** (administradores) manda à
Microsoft o mesmo pedido de novo: se ele tinha sido consumido, a Microsoft
confirma e as moedas entram; se não, ele é consumido agora. **Não credite
moedas à mão por um pedido stuck**: use o botão, que nunca credita duas vezes.

A compra pertence à **conta Microsoft** que está na Store, não à conta do
jogo: se duas contas do jogo usam o mesmo Windows e a mesma conta Microsoft,
um pacote pago e ainda não recebido vai para a conta do jogo que estiver
conectada quando a loja for aberta (sempre para uma só).

### 1. Criar os add-ons (Partner Center)

No produto do jogo: **Add-ons → Create a new add-on**, um para cada pacote e
um para o mês de VIP:

| Campo | Valor |
|---|---|
| Product type | **Developer-managed consumable** (consumível gerenciado pelo desenvolvedor) |
| Product ID | o código do pacote: `coins_500`, `coins_1200`, `coins_2800`, `coins_7000`; o do mês de VIP: `vip_month` (o servidor liga cada um pelo Store ID, então outro nome também serve) |
| Pricing | o preço de cada um (a Store converte para cada país) |
| Discoverability | **Hidden in the Microsoft Store** → **Available for purchase from within the parent product only**: a compra é feita dentro do jogo, com a conta do jogo conectada |
| Store listing | título e descrição em cada idioma (abaixo) |

Títulos e descrições (a descrição tem até 200 caracteres):

| Add-on | Português (Brasil) | English | Español |
|---|---|---|---|
| `coins_500` | 500 moedas | 500 coins | 500 monedas |
| `coins_1200` | 1300 moedas (1200 + 100 de bônus) | 1300 coins (1200 + 100 bonus) | 1300 monedas (1200 + 100 de bono) |
| `coins_2800` | 3200 moedas (2800 + 400 de bônus) | 3200 coins (2800 + 400 bonus) | 3200 monedas (2800 + 400 de bono) |
| `coins_7000` | 8500 moedas (7000 + 1500 de bônus) | 8500 coins (7000 + 1500 bonus) | 8500 monedas (7000 + 1500 de bono) |
| `vip_month` | 1 mês de VIP | 1 month of VIP | 1 mes de VIP |

- Pacotes, depois do número do título: "… para comprar itens cosméticos na
  loja do jogo: panos, tacos, giz e jogos de bolas. As moedas também vêm de
  jogar." / "… to buy cosmetic items in the game's shop: cloths, cues, chalk
  and ball sets. Coins also come from playing." / "… para comprar artículos
  cosméticos en la tienda del juego: paños, tacos, tiza y juegos de bolas.
  Las monedas también se ganan jugando."
- Mês de VIP: "Um mês de VIP: coroa ao lado do nome, itens exclusivos e 400
  moedas. Não se renova sozinho: para mais um mês, compre de novo." / "One
  month of VIP: a crown by your name, exclusive items and 400 coins. It
  doesn't renew by itself: buy again for another month." / "Un mes de VIP:
  una corona junto a tu nombre, artículos exclusivos y 400 monedas. No se
  renueva solo: compra de nuevo para otro mes."

Envie os add-ons para certificação. Depois de publicados, cada um tem um
**Store ID** de 12 caracteres (`9…`), em **Product identity** do add-on.

> O tipo precisa ser *developer-managed consumable*: o servidor ignora (e
> registra no log) add-ons de outro tipo.

#### O mês de VIP

- Cada compra dá **31 dias de VIP e as 400 moedas do mês**, como um mês pago
  pela Stripe. Se o jogador ainda é VIP, o mês novo começa quando o atual
  acaba: na aba VIP, o botão vira "Mais um mês".
- **Nada se renova sozinho**: a Store cobra uma vez por compra. (Um add-on
  do tipo *Subscription*, que renova, exigiria acompanhar renovações,
  cancelamentos e reembolsos pela API de assinaturas da Microsoft; não é
  usado.)
- No painel da equipe, a compra aparece em **Microsoft Store purchases** com
  o pacote `vip_month` e, em **Payments**, como VIP.
- Preço sugerido: perto do VIP mensal da Stripe (por exemplo US$ 2.99, e
  R$ 14,90 num grupo de mercado só do Brasil).

### 2. Aplicação no Microsoft Entra para o servidor

Esta aplicação fica no servidor do jogo e só serve para ler e consumir as
compras: não a use para mais nada.

1. Em [Microsoft Entra ID](https://entra.microsoft.com): **App registrations
   → New registration** (Web, sem URL). Anote o **Directory (tenant) ID** e o
   **Application (client) ID**; em **Certificates & secrets**, crie um
   *client secret* e anote o valor. A Microsoft pede que a conta tenha um
   *tenant* do Entra e permissão de administrador global nele.
2. No Partner Center, no produto do jogo: **Services → Product collections
   and purchases**, ponha o *Application (client) ID* num dos campos
   **Client ID**.
3. O *client secret* expira (no máximo em 2 anos): anote a data e troque a
   variável `SNOOK_LEGEND_MSSTORE_CLIENT_SECRET` antes disso.

### 3. Configurar o servidor (Render → Environment)

| Variável | Valor |
|---|---|
| `SNOOK_LEGEND_MSSTORE_TENANT_ID` | o *Directory (tenant) ID* |
| `SNOOK_LEGEND_MSSTORE_CLIENT_ID` | o *Application (client) ID* |
| `SNOOK_LEGEND_MSSTORE_CLIENT_SECRET` | o *client secret* (marque como secreto) |
| `SNOOK_LEGEND_MSSTORE_PRODUCTS` | `coins_500=9XXXXXXXXXXX;coins_1200=9XXXXXXXXXXX;coins_2800=9XXXXXXXXXXX;coins_7000=9XXXXXXXXXXX;vip_month=9XXXXXXXXXXX` |

No log de início deve aparecer
`Microsoft Store purchases on (4 packs, VIP month)`. Se a lista tiver um erro
(pacote desconhecido, Store ID inválido, repetido), o servidor **não** liga as
compras da Store e diz o motivo no log: um mapeamento errado creditaria o
pacote errado. Pode ligar só alguns add-ons: os que não estiverem na lista não
aparecem no jogo (sem `vip_month`, a aba VIP continua dizendo que o VIP não é
vendido nesta versão).

> **Só ponha `vip_month` na lista com o servidor novo no ar.** Um servidor de
> antes do mês de VIP (0.8.0) não conhece `vip_month`, recusa a lista inteira
> e desliga também a venda de moedas. Na dúvida, comece só com os pacotes e
> acrescente `;vip_month=…` depois do deploy.

**Ordem:** primeiro o servidor (com as migrações novas, que rodam sozinhas no
início), depois a versão do jogo. Um servidor sem estas variáveis faz a aba
Moedas da versão da Store voltar ao texto de antes; nada quebra. O mês de VIP
não tem migração; uma versão do jogo de antes dele (0.8.0) não o mostra e
ignora o campo novo das mensagens, então ele só é vendido pela versão nova.

### 4. Classificação etária e página da loja

- **IARC:** atualize o questionário dizendo que o jogo **vende itens digitais
  com dinheiro**. Os pacotes não têm sorteio (nada de *loot box*), então isso
  costuma só acrescentar o aviso "Compras no jogo" (*In-Game Purchases*), sem
  mudar a faixa etária. **Confira o resultado antes de enviar**: se a idade
  mudar, não publique esta versão e reavalie.
- **Política 10.8.4:** a descrição da loja deve dizer que há compras no jogo e
  a faixa de preços, por exemplo: "Opcional: pacotes de moedas para itens
  cosméticos, de R$ X a R$ Y, e um mês de VIP por R$ Z. As moedas também vêm
  de jogar."

### 5. Testar antes de abrir para todos

1. Publique a versão para um **público privado** (*Private audience*, só as
   contas Microsoft que você listar) ou use um *package flight*.
2. Com uma dessas contas, instale pela Store, entre no jogo e compre o pacote
   mais barato. As moedas devem chegar em segundos; o painel da equipe mostra
   o pedido como `done`.
3. Reembolse a compra de teste pelo Partner Center, se quiser, e tire as
   moedas pelo painel (**Shop…** com valor negativo); na lista de pagamentos,
   **Mark as test** separa essa compra da contagem.
4. Teste também fechar o jogo logo depois de pagar: ao abrir a loja de novo,
   as moedas chegam.
5. Compre o mês de VIP: a aba VIP passa a mostrar até quando, com o botão
   "Mais um mês", e o painel mostra o pedido `vip_month` como `done`.

### Reembolsos

Diferente do Stripe, a Microsoft **não avisa o servidor** de reembolsos ou
estornos: as moedas de uma compra reembolsada não são retiradas sozinhas,
nem o mês de VIP. Os ids de pedido (*order ids*) ficam gravados em cada
compra, para cruzar com o relatório do Partner Center e tirar as moedas (e,
pelo botão **VIP** do painel, os dias de VIP) quando for o caso. Uma próxima etapa pode automatizar isso (a API *Clawback* da
Microsoft, que exige uma fila no Azure).

## Lançar uma versão

Do mesmo jeito de sempre (veja `RELEASING.md`): versão no `Cargo.toml`,
seção no `CHANGELOG.md`, tag `vX.Y.Z`. O workflow *Microsoft Store* gera o
MSIX (a versão `0.1.2` vira `0.1.2.0`, como a Store exige) e o deixa como
o artefato `msix` da execução. O envio à Store é feito à mão:

1. Baixe o artefato `msix` da execução do workflow *Microsoft Store* da tag.
2. No Partner Center: o produto → **Start update** (nova submissão).
3. Em **Packages**, envie o `.msix` (e remova o da versão anterior).
4. Em **Store listings**, escreva as novidades da versão em **What's new in
   this version** (o texto de `crates/client/whatsnew/` serve).
5. **Submit to the Store**.

Betas (`v0.2.0-beta.1`) não vão para a Store.

## Testar o pacote no seu computador

Com o **Modo de desenvolvedor** ligado (Configurações → Sistema → Para
desenvolvedores), no PowerShell, na pasta do projeto:

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release --locked -p snook-legend --features store
./tools/msix/make-msix.ps1 -IdentityName SnookLegend.Teste -Publisher "CN=Teste" `
  -PublisherDisplayName "Teste" -DisplayName "Snook Legend (teste)" -Register
```

O jogo aparece no menu Iniciar como "Snook Legend (teste)". Para remover:
**Configurações → Aplicativos → Snook Legend (teste) → Desinstalar**.

## O nome

O nome que o jogador vê vem de `snook_legend_proto::GAME_NAME` (janela,
menu, salão, notificações, instalador, mensagens do servidor, páginas web).
Fora do código, ele precisa estar igual em dois lugares:

| Onde | O que fazer |
|---|---|
| Partner Center | **Product management → Product identity → Reserve more names**: reserve "Snook Legend" e use esse nome nas listagens e no pacote. |
| GitHub (**Settings → Secrets and variables → Actions → Variables**) | `MSSTORE_DISPLAY_NAME` = `Snook Legend`. A identidade do pacote (`MSSTORE_IDENTITY_NAME`, `MSSTORE_PUBLISHER`) **não muda**: é ela que faz a Store atualizar o jogo de quem já o tem. |
