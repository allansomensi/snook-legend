# Colocando o servidor do Snook Legend no ar (Render + Neon)

Este guia leva o servidor do zero até o jogo conectando nele. Leva uns 20
minutos. O servidor oficial roda no plano **Starter** do Render (o
`render.yaml` já pede esse plano); para testar, os planos gratuitos do Render
e do Neon também servem, com os limites abaixo.

```
 Jogadores ──wss──▶  Render (servidor Snook Legend, Docker)  ──TLS──▶  Neon (PostgreSQL)
                         └─ /admin  (painel da equipe)
```

- **Render** roda o servidor (o mesmo binário de sempre, em um container).
- **Neon** guarda contas, ratings, histórico, conquistas, amizades e denúncias.
- **Vercel não é necessária**: o painel administrativo é servido pelo próprio
  servidor em `/admin` (mesma origem, sem CORS, um deploy só). Se um dia você
  quiser um site do jogo, a Vercel é ótima para isso, e o servidor já expõe
  `GET /api/leaderboard?rules=brazilian|official` (o ranking de cada regra; a
  brasileira quando omitido) e `GET /api/status` (JSON público, com CORS) para o site
  mostrar o ranking e quantos estão online.

## O que saber sobre os planos

O servidor oficial usa o **Render Starter** (US$ 7/mês), que não dorme. Os
limites dos planos gratuitos, para quem for testar:

| | Limite | O que significa para o jogo |
|---|---|---|
| Render Free | dorme após 15 min sem tráfego; acorda em ~30–60 s | A primeira conexão do dia demora. O jogo avisa ("o servidor está acordando") e tenta de novo sozinho. Um plano pago não dorme. |
| Render Free | 750 h/mês por conta | Dá para 1 serviço ligado 24/7 (744 h). Não crie um segundo serviço grátis ligado. |
| Render Free | reinicia a cada deploy | Partidas em andamento são perdidas no deploy. Faça deploys em horários calmos. |
| Render Free | 512 MB RAM, CPU compartilhada | Suficiente para dezenas de jogadores simultâneos. A simulação das tacadas roda fora do lock principal. |
| Neon Free | 0,5 GB, suspende após 5 min parado | Centenas de milhares de partidas cabem. Acordar o banco leva <1 s. |

O Render Starter resolve os três primeiros itens sem mudar nada no código.
Para testar de graça, troque `plan: starter` por `plan: free` no
`render.yaml` (ou escolha o plano no painel do Render).

## 1. Banco de dados no Neon

1. Crie uma conta em <https://neon.tech> e um projeto (região **AWS US East
   (N. Virginia)**, a mesma do Render no passo 2, para a menor latência).
2. Em **Dashboard → Connection Details**, escolha o banco `neondb`, papel
   padrão, e **desmarque "Connection pooling"** (use a conexão **direta**, sem
   `-pooler` no host: o servidor mantém poucas conexões fixas e usa
   *prepared statements*).
3. Copie a string. Ela tem este formato:

   ```
   postgresql://usuario:senha@ep-xxxx-123456.us-east-1.aws.neon.tech/neondb?sslmode=require
   ```

Não precisa criar tabelas: o servidor aplica as migrations sozinho ao iniciar
(`crates/server/migrations`).

## 2. Servidor no Render

1. Crie uma conta em <https://render.com> e conecte seu GitHub.
2. **New → Blueprint**, escolha o repositório `snook-legend`. O Render lê o
   `render.yaml` da raiz e propõe o serviço `snook-legend-server` (Docker, plano Starter,
   região Virginia, health check em `/health`).
3. Ele vai pedir as variáveis marcadas como secretas:
   - `DATABASE_URL`: a string do Neon do passo 1.
   - `SNOOK_LEGEND_ADMINS`: seu nome de usuário no jogo (ex.: `allan`). Vários: `allan,maria`.
4. **Apply**. O primeiro build (Rust em release) leva ~10–15 min; os
   seguintes usam cache.
5. Quando ficar "Live", teste:

   ```bash
   curl https://snook-server.onrender.com/health
   ```

   Deve responder `ok`. (O nome exato da URL aparece no topo do serviço.)

As outras variáveis já vêm no `render.yaml`:

| Variável | Valor | Para quê |
|---|---|---|
| `SNOOK_LEGEND_CLIENT_IP_HEADER` | `cf-connecting-ip,true-client-ip,x-forwarded-for` | O Render fica atrás da Cloudflare: o servidor lê o IP real do jogador desses cabeçalhos. Sem isso todos parecem vir do mesmo IP e os limites anti-abuso (contas por IP, conexões por IP) bloqueariam gente inocente. O painel avisa se isso acontecer. |
| `SNOOK_LEGEND_TRUSTED_PROXIES` | (não precisa) | De quais proxies o servidor acredita nesses cabeçalhos. Sem a variável: só de redes privadas, que é de onde o proxy do Render conecta. Uma conexão vinda de outro lugar conta pelo próprio IP, diga o cabeçalho o que disser (ninguém escapa dos limites por IP inventando um cabeçalho). Só defina se o seu proxy vier de um IP público, por exemplo `203.0.113.0/24`. |
| `SNOOK_LEGEND_DB_POOL` | `5` | Conexões com o banco (o Neon grátis aceita bem). |
| `SNOOK_LEGEND_SHOT_CLOCK` | `30` | Segundos por tacada nas partidas online. |
| `SNOOK_LEGEND_MOTD` | (opcional) | Mensagem de boas-vindas do salão. |
| `RUST_LOG` | `info` | Nível dos logs (veja em **Logs** no Render). |

> Alternativa sem Blueprint: **New → Web Service → Existing image**, usando a
> imagem `ghcr.io/allansomensi/snook-legend-server:latest` publicada pelo workflow de
> release (passo 6). O deploy fica instantâneo porque não compila nada no Render.

## 3. Virar administrador

1. Abra o jogo, **Jogar online → Nova conta**, crie a conta com o nome que pôs
   em `SNOOK_LEGEND_ADMINS` (ou entre, se ela já existe). Ao entrar, ela vira admin.
2. Abra `https://snook-server.onrender.com/admin` e entre com o mesmo usuário e
   senha.

No painel: visão geral (online, partidas, contas, denúncias abertas), busca de
jogadores, ban temporário/permanente, silenciar, desconectar, encerrar todas
as sessões, mudar papel (jogador/moderador/admin), redefinir senha, fila de
denúncias com o trecho de chat anexado, partidas e torneios ao vivo, anúncio
no salão e o log de auditoria de toda ação da equipe. Moderadores podem
moderar; só administradores mudam papéis e senhas. Veja `docs/ADMIN.md`.

## 4. Apontar o jogo para o servidor

O endereço do jogo é a URL do Render com `wss://` e `/ws` no fim:

```
wss://snook-server.onrender.com/ws
```

- **Para testar agora**: no jogo, **Configurações → Jogo → Servidor**, cole o
  endereço.
- **Para distribuir**: compile com o endereço embutido, assim ninguém precisa
  configurar nada:

  ```bash
  SNOOK_LEGEND_SERVER=wss://snook-server.onrender.com/ws cargo build --release -p snook-legend
  ```

  No Windows (PowerShell):

  ```powershell
  $env:SNOOK_LEGEND_SERVER="wss://snook-server.onrender.com/ws"; cargo build --release -p snook-legend
  ```

## 5. Rede local (LAN)

Não precisa de servidor nenhum: em **Rede local**, um jogador clica em
**Hospedar uma partida neste computador** e os outros veem a partida na lista
(descoberta automática por broadcast UDP na porta 47878) ou entram pelo IP
mostrado na tela (porta 47879). Na primeira vez o Windows pergunta sobre o
firewall: permita em **redes privadas**. Na LAN não há contas nem rating; há
salão, chat, mesas, torneios e partidas como no online.

## 6. Publicar versões (GitHub Actions)

1. No GitHub: **Settings → Secrets and variables → Actions → Variables → New
   variable**: `SNOOK_LEGEND_SERVER` = `wss://snook-server.onrender.com/ws`.
2. Crie uma tag:

   ```bash
   git tag v0.1.0
   ```

   ```bash
   git push origin v0.1.0
   ```

3. O workflow **Release** (gerado pelo [dist](https://axodotdev.github.io/cargo-dist))
   compila o jogo para Windows, macOS (Intel e Apple Silicon) e Linux (x64 e
   ARM64) com o servidor embutido e publica na página de Releases o
   instalador `.msi`, os pacotes `.zip`/`.tar.xz` e os instaladores de uma
   linha (shell e PowerShell). O workflow **Server image** publica a imagem
   do servidor em `ghcr.io/<você>/snook-legend-server`. Detalhes em
   [RELEASING.md](RELEASING.md).

4. **Ponha o servidor novo no ar.** O Render não percebe sozinho que saiu
   uma imagem nova: enquanto ele rodar a versão anterior, quem atualizou o
   jogo não consegue jogar online. Para isso ser automático, crie o segredo
   do repositório `RENDER_DEPLOY_HOOK` (Settings → Secrets and variables →
   Actions → **New repository secret**) com a URL de **Render →
   snook-legend-server → Settings → Deploy Hook**. O workflow **Server image**
   chama essa URL depois de publicar a imagem. Sem o segredo, faça à mão:
   **Render → snook-legend-server → Manual Deploy → Deploy latest reference**.
   Confira em `https://<seu-serviço>.onrender.com/api/status`: `version`
   deve ser a nova.

## 7. Servidor sempre acordado

O servidor oficial roda no plano **Starter** do Render, que não dorme. No
plano gratuito o servidor dorme depois de 15 minutos sem tráfego e o
primeiro jogador espera cerca de um minuto (o workflow que o acordava a cada
10 minutos foi retirado do repositório).

## 8. Backups

O Neon grátis mantém histórico de 24 h (restauração a um ponto no tempo pelo
painel). Para uma cópia sua:

```bash
pg_dump "postgresql://usuario:senha@ep-xxxx.us-east-1.aws.neon.tech/neondb?sslmode=require" -Fc -f snook-legend-$(date +%F).dump
```

## 9. Loja: moedas e VIP com dinheiro real (Stripe)

Sem esta etapa a loja funciona com itens grátis, de conquista e VIP dados
pelo painel, mas não vende moedas nem VIP. Os pagamentos usam a página
segura da Stripe (cartão, Pix, boleto): o jogo nunca vê dados de cartão.

1. Crie uma conta em https://stripe.com (Brasil é suportado) e complete o
   cadastro da empresa ou pessoa física para receber pagamentos.
2. Comece no **modo de teste** (chave `sk_test_…`), teste tudo com o cartão
   `4242 4242 4242 4242`, e só então repita com as chaves de produção.
3. **Payment methods:** ative cartão e, se quiser, **Pix** e **boleto**.
4. **Products → Add product**, um produto para cada item abaixo, com preço
   em BRL:
   - `coins_500`, `coins_1200`, `coins_2800`, `coins_7000`: preço
     **único** (one-off). Sugestão: R$ 4,90 · 9,90 · 19,90 · 44,90.
   - `vip_monthly`: preço **recorrente mensal**. Sugestão: R$ 14,90.
   - `vip_gift` (opcional): um mês de VIP de presente para um amigo, preço
     **único** (one-off). Sugestão: R$ 14,90. Sem ele, o botão de presentear
     VIP não aparece.
   - `welcome_pack` (opcional): o pacote de boas-vindas (taco e pano
     exclusivos e 600 moedas, uma vez por conta), preço **único**. Sugestão:
     R$ 4,90. Sem ele, o pacote não aparece.
   Copie o **ID do preço** de cada um (começa com `price_`).
5. **Settings → Billing → Customer portal:** ative o portal e permita
   cancelar a assinatura (é por ele que o jogador gerencia o VIP).
6. **Developers → Webhooks → Add endpoint:**
   - URL: `https://snook-server.onrender.com/stripe/webhook`
   - Eventos: `checkout.session.completed`,
     `checkout.session.async_payment_succeeded`, `invoice.paid`,
     `customer.subscription.deleted`, `charge.refunded`,
     `charge.dispute.created`.
   Copie o **Signing secret** (`whsec_…`).
7. No **Render → snook-legend-server → Environment**, adicione:

   | Variável | Valor |
   |---|---|
   | `SNOOK_LEGEND_STRIPE_SECRET` | a chave secreta (`sk_test_…` ou `sk_live_…`) |
   | `SNOOK_LEGEND_STRIPE_WEBHOOK_SECRET` | o `whsec_…` do passo 6 |
   | `SNOOK_LEGEND_PUBLIC_URL` | `https://snook-server.onrender.com` |
   | `SNOOK_LEGEND_STRIPE_PRICES` | `coins_500=price_A\|R$ 4,90;coins_1200=price_B\|R$ 9,90;coins_2800=price_C\|R$ 19,90;coins_7000=price_D\|R$ 44,90;vip_monthly=price_V\|R$ 14,90/mês;vip_gift=price_G\|R$ 14,90;welcome_pack=price_W\|R$ 4,90` |

   O texto depois de `|` é o preço mostrado no jogo: mantenha igual ao da
   Stripe. Salve: o servidor reinicia e o log mostra `payments on (7 prices)`.
8. No jogo: **Loja → Moedas → Comprar**. Paga no navegador; as moedas
   chegam em segundos. No painel `/admin`, a ficha do jogador mostra a
   carteira, os pagamentos e o extrato.

Como funciona por dentro (e por que é seguro):
- Preço, saldo e posse de itens só existem no servidor; o jogo apenas pede.
- O webhook é aceito só com a assinatura da Stripe válida e recente.
- Cada crédito tem a referência do pagamento: se a Stripe repetir o aviso,
  nada é creditado duas vezes.
- Estorno ou contestação tiram as moedas de volta (o saldo pode ficar
  negativo, e então nada pode ser comprado até voltar ao positivo).
- VIP: cada mensalidade paga estende o VIP até o fim do período (+ algumas
  horas de folga) e dá as moedas do mês. Cancelado, vale até o fim do mês pago.

**Lojas de aplicativos:** a Steam e a Microsoft Store exigem os sistemas de
pagamento delas para compras dentro do jogo. A versão da Microsoft Store já
não oferece a Stripe e vende os pacotes de moedas pela própria Store (veja
[MSSTORE.md](MSSTORE.md), "Venda de moedas e VIP pela Store"). No site próprio,
itch.io e GitHub, a Stripe é permitida.

## 10. Termos de uso e política de privacidade

Ficam no **site do jogo** (repositório `snooklegend.app`, no Vercel), em três
línguas:

| Página | Português | English | Español |
|---|---|---|---|
| Termos de uso | `/pt/termos/` | `/terms/` | `/es/terminos/` |
| Política de privacidade | `/pt/privacidade/` | `/privacy/` | `/es/privacidad/` |

O texto fica em `src/content/legal/` no repositório do site (o português é o
texto de referência; a data no topo vem de `LEGAL_UPDATED` em
`src/lib/legal.ts`: mude a data junto com o texto). **Leia os textos antes
de usar**: eles descrevem o jogo como ele é hoje, mas não são uma revisão
jurídica.

Quem oferece o jogo vem das variáveis abaixo, no **Vercel → o projeto →
Settings → Environment Variables** (Production e Preview), e entra nas
páginas a cada deploy: nada pessoal fica no repositório. O Decreto
7.962/2013 (comércio eletrônico) pede que quem vende pela internet mostre
nome, CPF ou CNPJ, endereço e um contato eletrônico: com a loja ligada,
defina todas.

| Variável | O que é |
|---|---|
| `LEGAL_CONTROLLER` | seu nome ou o da sua empresa (quem oferece o jogo e controla os dados) |
| `LEGAL_DOCUMENT` | o CPF ou o CNPJ (sem ela, a linha não aparece) |
| `LEGAL_ADDRESS` | o endereço (sem ela, a linha não aparece) |
| `LEGAL_CONTACT` | o e-mail de atendimento: dúvidas, reembolsos, recursos e pedidos de privacidade (sem ele, as páginas apontam para as *issues* do GitHub) |

O servidor do jogo não guarda os textos: os endereços antigos dele
(`/termos`, `/terms`, `/terminos`, `/privacidade`, `/privacy`,
`/privacidad`) e a raiz `/` levam às páginas do site, com redirecionamento
permanente. Links antigos (das lojas, da Stripe, de versões antigas do jogo)
continuam valendo. O site é `https://snooklegend.app`
(`SNOOK_LEGEND_SITE_URL` troca, para outro servidor).

Onde usar os endereços:

- **Stripe → Settings → Business → Public details:** *Terms of service* =
  `https://snooklegend.app/pt/termos/`, *Privacy policy* =
  `https://snooklegend.app/pt/privacidade/`, o e-mail de suporte e a
  política de reembolso (o link dos termos: a seção 8 trata do direito de
  arrependimento de 7 dias). A página de pagamento da Stripe mostra esses
  links.
- **Microsoft Partner Center** (veja [MSSTORE.md](MSSTORE.md)):
  *Privacy policy URL* = `https://snooklegend.app/privacy/`, *Additional
  license terms* = o link de `/terms/`, e o e-mail em *Support contact info*.
- **Reembolsos:** um pedido de arrependimento (até 7 dias) é feito na Stripe
  (**Payments → o pagamento → Refund**). O servidor recebe o aviso
  `charge.refunded` e tira de volta as moedas, o VIP e os itens do pacote de
  boas-vindas sozinho.

## 11. Domínios próprios: site, API e painel

O jogo usa três endereços, cada um com um papel:

| Endereço | O que serve | Onde roda |
|---|---|---|
| `snooklegend.app` | o site: página do jogo, downloads, termos e privacidade | Vercel |
| `api.snooklegend.app` | o jogo: `/ws`, `/api/...`, webhook da Stripe | Render |
| `admin.snooklegend.app` | o painel da equipe (`/admin`) | Render (o mesmo serviço) |

A partir da 0.10.2 o jogo se conecta em `wss://api.snooklegend.app/ws`. Uma
sessão salva no endereço antigo (`snook-server.onrender.com`) vale no novo
(ninguém precisa entrar de novo), e se o domínio novo não responder o jogo
usa o antigo sozinho. Mesmo assim, **faça os passos abaixo antes de lançar
a 0.10.2**, nesta ordem.

### 11.1 Os termos e a privacidade no site

Publique o site com as páginas legais (o *merge* da branch no `main` do
repositório `snooklegend.app`) e defina as variáveis `LEGAL_*` no Vercel
(passo 10). Confira `https://snooklegend.app/pt/privacidade/`.

### 11.2 DNS (no Vercel)

Em **Vercel → Domains → snooklegend.app → DNS Records**, crie:

| Tipo | Nome | Valor |
|---|---|---|
| `CNAME` | `api` | `snook-server.onrender.com` |
| `CNAME` | `admin` | `snook-server.onrender.com` |

### 11.3 Domínios no Render

Em **Render → o serviço → Settings → Custom Domains**, adicione
`api.snooklegend.app` e `admin.snooklegend.app`. O Render confere o DNS e
emite os certificados. O endereço `snook-server.onrender.com` continua
funcionando (os jogos antigos usam ele).

Depois, em **Environment**, troque `SNOOK_LEGEND_PUBLIC_URL` para
`https://api.snooklegend.app` (as páginas de volta do pagamento) e, na
Stripe, o endereço do webhook para `https://api.snooklegend.app/stripe/webhook`
(o antigo também segue funcionando). As variáveis `SNOOK_LEGEND_GAME_NAME`,
`SNOOK_LEGEND_PRIVACY_CONTROLLER`, `SNOOK_LEGEND_LEGAL_DOCUMENT`,
`SNOOK_LEGEND_LEGAL_ADDRESS` e `SNOOK_LEGEND_PRIVACY_CONTACT` não são mais
usadas pelo servidor: apague-as depois de levar os valores para o Vercel.

### 11.4 O painel no endereço dele

Com o DNS funcionando, confira que `https://admin.snooklegend.app/admin` abre
o painel e então, em **Render → Environment**, defina
`SNOOK_LEGEND_ADMIN_HOST` = `admin.snooklegend.app`: o painel passa a
responder só nesse endereço (em qualquer outro, "não encontrado"), e a raiz
desse endereço abre o painel.

### 11.5 As lojas

Atualize os endereços no **Microsoft Partner Center** e na **Stripe**
(passo 10).

## Problemas comuns

| Sintoma | Causa provável |
|---|---|
| Jogo fica em "Conectando ao servidor…" ~1 min e entra | Instância do plano grátis acordando (no Starter isso não acontece). |
| `could not start: … password authentication failed` nos logs | `DATABASE_URL` errada. Copie de novo do Neon. |
| `… prepared statement … already exists` | Usou a string com `-pooler`. Use a conexão direta. |
| Jogadores recebem "Muitas contas novas desta rede" sem motivo | `SNOOK_LEGEND_CLIENT_IP_HEADER` ausente. O painel mostra o aviso "Every player comes from the same address". |
| "Saiu uma nova versão do jogo" com o jogo já atualizado | O servidor ainda roda a versão anterior: faça o deploy da imagem nova (passo 6.4). A partir da 0.3.1 o jogo percebe isso sozinho e diz que o servidor está sendo atualizado. |
| "Esta versão do Snook Legend está desatualizada" | O jogo é mais antigo que o servidor: atualize o jogo. |
| Partida em andamento sumiu | Deploy ou o Render reiniciou a instância (o salão e as partidas ficam na memória; contas e resultados ficam no banco). |
