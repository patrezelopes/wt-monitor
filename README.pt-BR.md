# wt-monitor

Um pequeno painel local que mostra **o que cada um dos seus worktrees do Git está fazendo**: CPU, memória, threads, afinidade de CPU e — se você usa o [Claude Code](https://claude.com/claude-code) — no que cada sessão de agente está trabalhando e quando ela termina uma tarefa.

Um único binário, **sem dependências** (só a biblioteca padrão do Rust), só Linux (lê o `/proc`). A interface é um único HTML embutido no binário.

> 🇬🇧 [Read in English](README.md)

| Visão geral (escuro) | Visão geral (claro) |
|---|---|
| ![visão geral](docs/img/overview.png) | ![visão geral, claro](docs/img/overview-light.png) |

| Detalhe da sessão: agentes, skill e quem fala com quem | Arena (o visual divertido) |
|---|---|
| ![sessão](docs/img/session.png) | ![arena](docs/img/arena.png) |

| Terminais iniciados pelo painel (opt-in), quatro lado a lado |
|---|
| ![terminais](docs/img/terminals.png) |

## Recursos

- **Visão por worktree** – os processos são agrupados pelo diretório mais próximo que contém `.git` (arquivo nos worktrees ligados, pasta nos repositórios normais). Branch, status, CPU %, memória, threads, afinidade e gráficos de 60 s.
- **Agentes, skills e subagentes** – nos worktrees com Claude Code mostra quais agentes estão rodando (o principal e cada subagente em execução, com tipo e modelo), a **skill** em uso, a ferramenta em que cada subagente está e **quem fala com quem** (delegações, `SendMessage`, conclusões). Cada worktree recebe um status derivado: *trabalhando*, *aguardando você*, *ocioso* ou *no limite* (veja [Como o status é derivado](docs/architecture.md#status)).
- **Detalhe da sessão** – clique num worktree para abrir uma gaveta com abas: visão geral, agentes (topologia + conversa entre agentes), feed de atividade ao vivo (pedidos, mensagens do agente, ferramentas e resultados) e processos.
- **Dois visuais, um botão** – *Visão geral*, uma tabela densa no estilo dos painéis modernos (escuro e claro, segue o sistema), e *Arena*, em que cada worktree é um robozinho com seus subagentes ligados a ele e pacotes de mensagem viajando entre eles. Digita quando está ocupado, dorme quando está parado, comemora ao terminar uma tarefa e ganha XP/níveis. Respeita `prefers-reduced-motion`.
- **Terminais (opt-in)** – abra terminais de verdade — um shell de login, ou o `claude` — dentro do painel, agrupados por worktree e exibidos 1, 2 ou 4 por vez. Só terminais **iniciados pelo painel** são gerenciados. Veja [Terminais](#terminais-opt-in).
- **Filtros, busca e fixados** – filtre por status, busque por worktree, branch, agente ou skill (`Ctrl K`) e marque com ★ os worktrees que importam; eles ficam no topo mesmo sem processos.
- **Notificações do desktop** (via `notify-send`) para CPU/memória altas em um worktree, memória alta do sistema e **quando um agente termina uma tarefa**, com a última mensagem dele.
- **Barras de CPU por core**, memória do sistema e load.

## Como isso se diferencia?

O wt-monitor ocupa um lugar diferente de ferramentas como o [Orca](https://github.com/stablyai/orca) e o [Compozy](https://github.com/compozy/compozy). Elas são ambientes de trabalho e orquestradores: você roda seus agentes *dentro* delas (o Orca é um IDE desktop com um worktree por tarefa; o Compozy é um daemon dono das sessões, tarefas e automações). O wt-monitor é um **observador** leve:

- Ele acompanha os agentes que você **já roda**, em qualquer terminal, sem mudar o seu jeito de trabalhar.
- Ele mostra o que os agentes fazem (subagentes, skills, quem fala com quem) **ao lado do que a máquina faz** (CPU, memória e afinidade de CPU por worktree).
- É um único binário, sem dependências de Rust, sem requisições de rede e com o servidor só em loopback.
- Só é dono dos terminais que **ele mesmo inicia**, e só se você habilitar.

O que ele deliberadamente **não** tenta ser: editor, navegador embutido, revisor de diff, cliente SSH, aplicativo móvel ou agendador. Se você precisa disso, use uma ferramenta feita para isso; nada impede usar as duas (não testamos).

Esta comparação se baseia nos READMEs e na documentação públicos dos projetos em outubro de 2026. Se algo estiver desatualizado, [abra uma issue](https://github.com/patrezelopes/wt-monitor/issues).

## Início rápido

Requisitos: Linux, Rust (`cargo`) e `notify-send` (pacote `libnotify-bin`) para as notificações.

```bash
git clone https://github.com/patrezelopes/wt-monitor && cd wt-monitor
make run                                # monitora ~/dev na porta 8765
make run ROOT=~/code PORT=9000          # ou escolha a pasta raiz e a porta
```

Abra <http://127.0.0.1:8765> (`make open`). Para ver a interface com dados fictícios, acrescente `?demo` — por exemplo <http://127.0.0.1:8765/?demo&mode=fun>.

Tudo é um alvo do `make` — veja [Alvos do make](#alvos-do-make) ou rode `make help`.

### Iniciar com o Ubuntu (serviço de usuário do systemd)

```bash
make install            # compila, instala em ~/.local/bin e ativa + inicia o serviço de usuário
make status             # está rodando?   (make logs para acompanhar os logs)
make linger             # opcional: sobe no boot, antes do login
```

`make install ROOT=~/code PORT=9000` grava outra raiz/porta na unit. Para remover: `make uninstall`.

## Notificações dos agentes (hook do Claude Code)

O Claude Code pode rodar um comando quando um agente termina de responder (o hook `Stop`). O `hook/wt-agent-hook` envia a última mensagem do agente ao painel, que a exibe e dispara uma notificação.

```bash
make hooks                              # todas as pastas ~/.claude e ~/.claude-*  (idempotente)
make hooks CONFIG_DIRS=~/.claude        # ou só as pastas indicadas
make hook-test                          # evento falso de "tarefa concluída", para ver funcionando
make hooks-uninstall                    # remove o hook
```

O script é copiado para `~/.local/bin/wt-agent-hook` e um hook `Stop` é adicionado a cada `settings.json`, com backup `settings.json.bak-wt-monitor` antes. Sessões do Claude já abertas podem precisar ser reiniciadas. Se o painel estiver fora do ar, o hook sai em silêncio e nunca bloqueia o agente. Veja [docs/agent-hook.md](docs/agent-hook.md).

## Configuração

Linha de comando: `wt-monitor [raiz=~/dev] [porta=8765]` (ou `ROOT=`/`PORT=` com `make`). Os limites de alerta são variáveis de ambiente (use `Environment=` na unit do systemd):

| Variável | Padrão | Significado |
|---|---|---|
| `WT_CPU_PCT` | `300` | Limite de CPU por worktree (100 = um core) |
| `WT_CPU_SECS` | `20` | Segundos acima do limite de CPU antes de alertar |
| `WT_MEM_GB` | `4` | Limite de memória por worktree (soma do RSS) |
| `WT_SYS_MEM_PCT` | `90` | Limite de uso de memória do sistema |

Cada alerta dispara uma vez, rearma quando o valor volta ao normal e espera pelo menos 2 minutos antes de repetir. O hook respeita `WT_PORT` se você mudou a porta.

Parâmetros de URL: `?mode=sober|fun`, `?theme=dark|light`, `?demo` e, com `?demo`, também `?open=<nome do worktree>&tab=agents|activity|procs|overview`.

## Terminais (opt-in)

O painel pode abrir terminais por conta própria. Isso vem **desligado por padrão**: permite que uma página web inicie shells como você, então leia antes o [SECURITY.md](SECURITY.md).

```bash
make run TERMINALS=1          # ou: wt-monitor --terminals   (WT_TERMINALS=1 também funciona)
make install TERMINALS=1      # o mesmo, como serviço de usuário do systemd
make open-terminals           # abre o painel com o token de uso único no link
```

Um segredo aleatório de 256 bits é criado no primeiro uso em `~/.config/wt-monitor/token` (modo `0600`) e nunca é devolvido pela API. O `make open-terminals` abre `http://127.0.0.1:8765/#token=…` uma vez; o navegador guarda o token (o fragmento nunca é enviado ao servidor) e o link some da barra de endereço. Sem o token, a aba *Terminais* explica como obtê-lo e os botões ficam desativados.

- **Aba "Terminais"**: escolha um worktree e clique em *Novo terminal* (um shell de login naquele worktree, então valem o seu `PATH`, o `nvm` e a troca de conta por pasta; usa `/bin/bash` quando `$SHELL` não está definido, como nos serviços de usuário do systemd) ou *Novo Claude* (o mesmo shell, com `claude` já digitado). A gaveta da sessão tem os mesmos dois botões.
- **Conjuntos**: os terminais ficam agrupados por worktree em abas; o seletor `1 / 2 / 4` mostra essa quantidade lado a lado. Fechar uma aba encerra o grupo de processos do terminal.
- **Reconexão**: os terminais vivem no servidor, não na página. Recarregue ou reabra o navegador e eles voltam com os últimos 256 KB de saída; os encerrados continuam listados por 10 minutos com o código de saída. No máximo 16 ao mesmo tempo.
- Só terminais **iniciados pelo painel** são gerenciados; ele nunca se conecta a terminais que você abriu em outro lugar.

O PTY e o WebSocket são implementados só com a biblioteca padrão (sem crates); o emulador de terminal é o [xterm.js](vendor/xterm/README.md), embutido no binário.

## Alvos do make

`make` sem argumentos mostra esta lista. Variáveis que você pode sobrescrever: `ROOT` (padrão `~/dev`), `PORT` (`8765`), `PREFIX` (`~/.local`), `TERMINALS` (`1` habilita os terminais), `CONFIG_DIRS` (pastas de config do Claude para o hook; padrão: todas).

| Alvo | O que faz |
|---|---|
| `make build` | Compila o binário otimizado |
| `make run` | Roda em primeiro plano em `ROOT`/`PORT` (`TERMINALS=1` habilita os terminais) |
| `make open-terminals` | Abre o painel com o token de uso único dos terminais no link |
| `make open` | Abre o painel no navegador padrão |
| `make test` | Testes unitários |
| `make fmt` / `make fmt-check` | Formata / confere a formatação |
| `make lint` | `clippy`, avisos viram erros |
| `make js-check` | Confere a sintaxe do script da página (precisa do `node`) |
| `make check` | `fmt-check`, `lint`, `test`, `js-check` |
| `make e2e` | Testes de ponta a ponta dos terminais, por HTTP/WebSocket e num Chrome headless de verdade (o CI roda `check` e `e2e`) |
| `make install` | Compila, instala em `PREFIX/bin`, ativa e (re)inicia o serviço de usuário |
| `make uninstall` | Para e remove o serviço e o binário |
| `make restart` / `status` / `logs` | Gerencia o serviço (`logs` acompanha o `journalctl`) |
| `make linger` | Sobe o serviço no boot, antes do login |
| `make hooks` | Instala o hook `Stop` do Claude Code (faz backup de cada `settings.json`) |
| `make hooks-uninstall` | Remove o hook |
| `make hook-test` | Envia um evento falso de "tarefa concluída" ao painel em execução |
| `make screenshots` | Regenera `docs/img/*.png` com os dados de demonstração (precisa do Chrome) |
| `make clean` | Remove os artefatos de build |

## Um worktree por conjunto de cores

O painel mostra a afinidade; configurá-la é uma linha, e os processos filhos herdam:

```bash
cd ~/dev/wt-a && taskset -c 0,1 claude
cd ~/dev/wt-b && taskset -c 2,3 claude
# ou com cgroup:  systemd-run --user --scope -p AllowedCPUs=2-3 bash -c 'cd ~/dev/wt-b && claude'
```

Ferramentas de build dimensionam seus workers pelo total de CPUs da máquina, não pela afinidade — limite-as também (`make -j2`, `jest --maxWorkers=2`).

## Segurança e privacidade

- Os terminais, quando habilitados, são protegidos por um token que a API nunca expõe, verificações de `Origin`/`Host` e um conjunto fechado de comandos — detalhes e modelo de ameaças no [SECURITY.md](SECURITY.md).
- Escuta **somente em 127.0.0.1** e rejeita qualquer cabeçalho `Host` diferente de `127.0.0.1:<porta>` / `localhost:<porta>` (proteção contra DNS rebinding).
- `POST /event` exige o cabeçalho `X-WT: 1`, que força um preflight de CORS; outras páginas abertas no navegador não conseguem injetar eventos falsos.
- O preview e as telas de agentes leem as transcrições do Claude Code (e as dos subagentes) em `~/.claude*/projects/` **apenas de worktrees dentro da raiz monitorada**; o caminho pedido é validado e nunca vira leitura de arquivo arbitrário. Transcrições podem ter conteúdo sensível — o preview é tão privado quanto a sua interface de loopback.
- Fixados e XP ficam no `localStorage` do navegador; nada é enviado para fora.
- Os detalhes vêm de `/proc/<pid>/{cwd,stat,status,cmdline}`; você só vê os processos que tem permissão de inspecionar (os seus).

## Como funciona

Veja [docs/architecture.md](docs/architecture.md).

## Limitações

Só Linux. Os terminais executam o seu shell de login e são limitados a 16; não se conectam a sessões que você iniciou em outro lugar. A memória é a soma do RSS, então páginas compartilhadas entre processos são contadas mais de uma vez. A detecção de tarefas usa o hook `Stop` do Claude Code, que dispara após *toda* resposta concluída, não só em tarefas longas. A detecção de agentes, skills e *aguardando você* é **inferida dos arquivos de transcrição** (o formato não é uma API estável) e é uma heurística de melhor esforço. A interface usa Inter / JetBrains Mono quando instaladas e cai para as fontes do sistema; não faz nenhuma requisição de rede.

## Contribuir e licença

PRs são bem-vindos — veja o [CONTRIBUTING.md](CONTRIBUTING.md). Licença [MIT](LICENSE).
