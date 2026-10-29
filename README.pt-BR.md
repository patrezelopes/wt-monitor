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
cargo run --release -- ~/dev 8765      # pasta raiz a monitorar, porta
```

Abra <http://127.0.0.1:8765>. Para ver a interface com dados fictícios, acrescente `?demo` — por exemplo <http://127.0.0.1:8765/?demo&mode=fun>.

### Iniciar com o Ubuntu (serviço de usuário do systemd)

```bash
./install.sh            # compila, instala em ~/.local/bin e ativa o serviço de usuário
systemctl --user status wt-monitor
```

Para subir no boot, antes do login: `loginctl enable-linger $USER`.

## Notificações dos agentes (hook do Claude Code)

O Claude Code pode rodar um comando quando um agente termina de responder (o hook `Stop`). O `hook/wt-agent-hook` envia a última mensagem do agente ao painel, que a exibe e dispara uma notificação.

```bash
python3 install-hooks.py                # todas as pastas ~/.claude, ~/.claude-*  (idempotente)
python3 install-hooks.py ~/.claude      # ou só as pastas indicadas
```

O script é copiado para `~/.local/bin/wt-agent-hook` e um hook `Stop` é adicionado a cada `settings.json`, com backup `settings.json.bak-wt-monitor` antes. Sessões do Claude já abertas podem precisar ser reiniciadas. Se o painel estiver fora do ar, o hook sai em silêncio e nunca bloqueia o agente. Veja [docs/agent-hook.md](docs/agent-hook.md).

## Configuração

Linha de comando: `wt-monitor [raiz=~/dev] [porta=8765]`. Os limites de alerta são variáveis de ambiente (use `Environment=` na unit do systemd):

| Variável | Padrão | Significado |
|---|---|---|
| `WT_CPU_PCT` | `300` | Limite de CPU por worktree (100 = um core) |
| `WT_CPU_SECS` | `20` | Segundos acima do limite de CPU antes de alertar |
| `WT_MEM_GB` | `4` | Limite de memória por worktree (soma do RSS) |
| `WT_SYS_MEM_PCT` | `90` | Limite de uso de memória do sistema |

Cada alerta dispara uma vez, rearma quando o valor volta ao normal e espera pelo menos 2 minutos antes de repetir. O hook respeita `WT_PORT` se você mudou a porta.

Parâmetros de URL: `?mode=sober|fun`, `?demo`, `?open=<nome do worktree>` (com `?demo`).

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
