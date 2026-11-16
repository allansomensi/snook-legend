# Microsoft Store

O jogo vai para a Microsoft Store como um pacote **MSIX**, feito do mesmo
código das outras versões. A Store assina o pacote, instala e atualiza
sozinha. O instalador `.msi` e os arquivos do GitHub continuam como estão.

## O que muda na versão da Store

O build da Store usa a feature `store` do jogo (`crates/client/src/channel.rs`):

- **Moedas compradas pela própria Store, nunca pelo Stripe.** A política da
  Store (10.8.1) pede que jogos vendam itens digitais pelo sistema de compras
  da Store. Os pacotes de moedas viram *add-ons* da Store (veja
  [Venda de moedas pela Store](#venda-de-moedas-pela-store)); enquanto o
  servidor não estiver configurado para isso, a aba Moedas explica que as
  moedas vêm de jogar, como antes. O VIP e o pacote de boas-vindas não são
  vendidos nesta versão. Moedas e itens comprados em outra versão continuam
  valendo (a política permite usar o que já foi comprado). Quem já é VIP
  continua vendo até quando, mas sem o botão de gerenciar a assinatura.
- **Runtime do C embutido** (`+crt-static`): o `.exe` não depende do Visual
  C++ Redistributable.
- **Firewall da rede local já liberado** pelo pacote (portas 47879/TCP e
  47878/UDP, só em redes privadas e de domínio): hospedar uma partida na rede
  local não mostra o aviso do Windows.

| Arquivo | O que é |
|---|---|
| `crates/client/msix/AppxManifest.xml` | o manifesto do pacote (identidade, ícones, firewall) |
| `crates/client/msix/Assets/` | os ícones do pacote (`cargo test --release -p snookered export_msix_assets -- --ignored` os refaz a partir do emblema) |
| `tools/msix/make-msix.ps1` | preenche o manifesto e monta o `.msix` |
| `.github/workflows/msstore.yml` | gera o MSIX (o envio à Store é manual) |
| `crates/server/pages/` | a política de privacidade (`/privacidade` e `/privacy`) |

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

### 2. Política de privacidade

O servidor publica a política em `https://<seu-servidor>/privacidade`
(português) e `/privacy` (inglês). Antes de usá-la, **leia o texto** em
`crates/server/pages/` e ajuste o que for preciso. Ele descreve o que o jogo
guarda hoje, mas não é uma revisão jurídica.

Na Render (**Environment**), defina:

| Variável | Exemplo |
|---|---|
| `SNOOKERED_GAME_NAME` | o nome do jogo na Store |
| `SNOOKERED_PRIVACY_CONTROLLER` | seu nome ou o da sua empresa |
| `SNOOKERED_PRIVACY_CONTACT` | um e-mail para pedidos de privacidade (sem ele, a página aponta para as *issues* do GitHub) |

No Partner Center, use `https://<seu-servidor>/privacy` como **Privacy policy
URL**.

### 3. Servidor sempre acordado

No plano gratuito da Render o servidor dorme e o primeiro jogador espera
cerca de um minuto. Defina a variável do repositório `SNOOKERED_HEALTH_URL` como
`https://<seu-servidor>/health` (o workflow *Keep the server awake* passa a
acordá-lo a cada 10 minutos) ou use um plano pago.

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
     [Venda de moedas pela Store](#venda-de-moedas-pela-store)).
   - **Packages:** envie o `.msix` baixado.
   - **Store listings** (português e inglês): descrição, capturas de tela
     (as de `docs/screenshots/` servem; o mínimo é 1366×768) e o link de
     suporte.
3. Envie para certificação (até três dias úteis).

## Venda de moedas pela Store

Os quatro pacotes de moedas (`coins_500`, `coins_1200`, `coins_2800`,
`coins_7000`) são vendidos como *add-ons* da Store. O dinheiro fica com a
Microsoft (que repassa pelo Partner Center, menos a comissão dela); o jogo
nunca vê dados de pagamento.

**Roteiro, na ordem** (detalhes de cada passo abaixo):

1. Partner Center: criar os 4 *add-ons* como *developer-managed
   consumable* e enviá-los para certificação; anotar o Store ID de cada um.
2. Microsoft Entra: registrar uma aplicação para o servidor, criar um
   *client secret* e pôr o *Application (client) ID* em **Services →
   Product collections and purchases** do jogo.
3. Render: definir as quatro variáveis `SNOOKERED_MSSTORE_*` e publicar o
   servidor novo (as migrações 0013 e 0014 rodam sozinhas); conferir no
   log `Microsoft Store purchases on (4 packs)`.
4. Atualizar o questionário IARC (compras no jogo) e a descrição da loja
   (faixa de preços).
5. Gerar o MSIX da versão nova, publicá-lo para um público privado e testar
   uma compra de verdade; depois liberar para todos.

### Como funciona (e por que é seguro)

1. O jogador clica num pacote; o jogo abre a **janela de compra da própria
   Store**. O jogo nunca decide sozinho que algo foi comprado.
2. Depois da compra, o servidor dá ao jogo um *ticket* (um token da sua
   aplicação no Microsoft Entra que só serve para a Store criar uma chave)
   e um identificador anônimo do jogador. A Store, no computador do jogador,
   cria com isso uma **chave da Microsoft Store** e o jogo a manda ao servidor.
3. O servidor confere que a chave foi feita para aquele jogador, **pergunta à
   Microsoft** quais pacotes aquela conta Microsoft comprou e ainda não
   recebeu e, para cada um, grava um pedido com um `trackingId` próprio
   **antes** de pedir à Microsoft que o consuma. As moedas só entram depois
   que a Microsoft confirma, na mesma transação do banco que fecha o pedido.
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

No produto do jogo: **Add-ons → Create a new add-on**, um para cada pacote:

| Campo | Valor |
|---|---|
| Product type | **Developer-managed consumable** (consumível gerenciado pelo desenvolvedor) |
| Product ID | o código do pacote: `coins_500`, `coins_1200`, `coins_2800`, `coins_7000` |
| Pricing | o preço de cada pacote (a Store converte para cada país) |
| Store listing | título (ex.: "500 moedas"; "1300 moedas (1200 + 100 de bônus)") e descrição |

Envie os add-ons para certificação. Depois de publicados, cada um tem um
**Store ID** de 12 caracteres (`9…`), em **Product identity** do add-on.

> O tipo precisa ser *developer-managed consumable*: o servidor ignora (e
> registra no log) pacotes de outro tipo.

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
   variável `SNOOKERED_MSSTORE_CLIENT_SECRET` antes disso.

### 3. Configurar o servidor (Render → Environment)

| Variável | Valor |
|---|---|
| `SNOOKERED_MSSTORE_TENANT_ID` | o *Directory (tenant) ID* |
| `SNOOKERED_MSSTORE_CLIENT_ID` | o *Application (client) ID* |
| `SNOOKERED_MSSTORE_CLIENT_SECRET` | o *client secret* (marque como secreto) |
| `SNOOKERED_MSSTORE_PRODUCTS` | `coins_500=9XXXXXXXXXXX;coins_1200=9XXXXXXXXXXX;coins_2800=9XXXXXXXXXXX;coins_7000=9XXXXXXXXXXX` |

No log de início deve aparecer `Microsoft Store purchases on (4 packs)`. Se a
lista tiver um erro (pacote desconhecido, Store ID inválido, repetido), o
servidor **não** liga as compras da Store e diz o motivo no log: um
mapeamento errado creditaria o pacote errado. Pode ligar só alguns pacotes:
os que não estiverem na lista não aparecem no jogo.

**Ordem:** primeiro o servidor (com a migração 0011, que roda sozinha no
início), depois a versão do jogo. Um servidor sem estas variáveis faz a aba
Moedas da versão da Store voltar ao texto de antes; nada quebra.

### 4. Classificação etária e página da loja

- **IARC:** atualize o questionário dizendo que o jogo **vende itens digitais
  com dinheiro**. Os pacotes não têm sorteio (nada de *loot box*), então isso
  costuma só acrescentar o aviso "Compras no jogo" (*In-Game Purchases*), sem
  mudar a faixa etária. **Confira o resultado antes de enviar**: se a idade
  mudar, não publique esta versão e reavalie.
- **Política 10.8.4:** a descrição da loja deve dizer que há compras no jogo e
  a faixa de preços, por exemplo: "Opcional: pacotes de moedas para itens
  cosméticos, de R$ X a R$ Y. As moedas também vêm de jogar."

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

### Reembolsos

Diferente do Stripe, a Microsoft **não avisa o servidor** de reembolsos ou
estornos: as moedas de uma compra reembolsada não são retiradas sozinhas.
Os ids de pedido (*order ids*) ficam gravados em cada compra, para cruzar com
o relatório do Partner Center e tirar as moedas pelo painel quando for o
caso. Uma próxima etapa pode automatizar isso (a API *Clawback* da
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
cargo build --release --locked -p snookered --features store
./tools/msix/make-msix.ps1 -IdentityName Snookered.Teste -Publisher "CN=Teste" `
  -PublisherDisplayName "Teste" -DisplayName "Snookered (teste)" -Register
```

O jogo aparece no menu Iniciar como "Snookered (teste)". Para remover:
**Configurações → Aplicativos → Snookered (teste) → Desinstalar**.

## O nome

O nome que o jogador vê vem de `snookered_proto::GAME_NAME` (janela, menu,
salão, notificações, mensagens do servidor, páginas web). Na Store, ele vem
das variáveis `MSSTORE_DISPLAY_NAME` (GitHub) e `SNOOKERED_GAME_NAME` (Render,
para a política de privacidade): use **Snookered** nas duas.
