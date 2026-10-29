# wt-monitor

Um pequeno painel local que mostra **o que cada um dos seus worktrees do Git está fazendo**: CPU, memória, threads, afinidade de CPU e — se você usa o [Claude Code](https://claude.com/claude-code) — no que cada sessão de agente está trabalhando e quando ela termina uma tarefa.

Um único binário, **sem dependências** (só a biblioteca padrão do Rust), só Linux (lê o `/proc`). A interface é um único HTML embutido no binário.

> 🇬🇧 [Read in English](README.md)

| Visual sóbrio | Visual divertido |
|---|---|
| ![sóbrio](docs/img/sober.png) | ![divertido](docs/img/fun.png) |

## Recursos

- **Visão por worktree** – os processos são agrupados pelo diretório mais próximo que contém `.git` (arquivo nos worktrees ligados, pasta nos repositórios normais). CPU %, memória, threads, afinidade, o core em que cada processo rodou por último e gráficos de 60 s.
- **Preview da sessão** – clique num worktree para abrir uma gaveta com os processos e o feed ao vivo da sessão mais recente do Claude Code nele (pedidos, mensagens do agente, chamadas de ferramenta e resultados).
- **Dois visuais, um botão** – um painel sóbrio e um divertido em que cada worktree é um robozinho: digita quando está ocupado, dorme quando está parado, comemora quando termina uma tarefa, transpira quando passa de um limite e ganha XP/níveis a cada tarefa concluída. Respeita `prefers-reduced-motion`.
- **Fixados** – marque com ★ os worktrees que importam; eles ficam no topo (mesmo sem processos) e o filtro "só fixados" esconde o resto.
- **Notificações do desktop** (via `notify-send`) para CPU/memória altas em um worktree, memória alta do sistema e **quando um agente termina uma tarefa**, com a última mensagem dele.
- **Barras de CPU por core**, memória do sistema e load.

![gaveta de preview](docs/img/preview.png)

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

Parâmetros de URL: `?mode=sober|fun`, `?demo`, `?open=<nome do worktree>` (com `?demo`).

## Alvos do make

`make` sem argumentos mostra esta lista. Variáveis que você pode sobrescrever: `ROOT` (padrão `~/dev`), `PORT` (`8765`), `PREFIX` (`~/.local`), `CONFIG_DIRS` (pastas de config do Claude para o hook; padrão: todas).

| Alvo | O que faz |
|---|---|
| `make build` | Compila o binário otimizado |
| `make run` | Roda em primeiro plano em `ROOT`/`PORT` |
| `make open` | Abre o painel no navegador padrão |
| `make test` | Testes unitários |
| `make fmt` / `make fmt-check` | Formata / confere a formatação |
| `make lint` | `clippy`, avisos viram erros |
| `make js-check` | Confere a sintaxe do script da página (precisa do `node`) |
| `make check` | Tudo que o CI roda: `fmt-check`, `lint`, `test`, `js-check` |
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

- Escuta **somente em 127.0.0.1** e rejeita qualquer cabeçalho `Host` diferente de `127.0.0.1:<porta>` / `localhost:<porta>` (proteção contra DNS rebinding).
- `POST /event` exige o cabeçalho `X-WT: 1`, que força um preflight de CORS; outras páginas abertas no navegador não conseguem injetar eventos falsos.
- O preview lê as transcrições do Claude Code em `~/.claude*/projects/` **apenas de worktrees dentro da raiz monitorada**; o caminho pedido é validado e nunca vira leitura de arquivo arbitrário. Transcrições podem ter conteúdo sensível — o preview é tão privado quanto a sua interface de loopback.
- Fixados e XP ficam no `localStorage` do navegador; nada é enviado para fora.
- Os detalhes vêm de `/proc/<pid>/{cwd,stat,status,cmdline}`; você só vê os processos que tem permissão de inspecionar (os seus).

## Como funciona

Veja [docs/architecture.md](docs/architecture.md).

## Limitações

Só Linux. A memória é a soma do RSS, então páginas compartilhadas entre processos são contadas mais de uma vez. A detecção de tarefas usa o hook `Stop` do Claude Code, que dispara após *toda* resposta concluída, não só em tarefas longas.

## Contribuir e licença

PRs são bem-vindos — veja o [CONTRIBUTING.md](CONTRIBUTING.md). Licença [MIT](LICENSE).
