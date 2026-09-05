# AGENTS.md

Este arquivo define as regras de desenvolvimento para todo o repositório MyLib. Ele se aplica
ao backend Rust, ao frontend React, às migrations, aos testes, aos scripts e à documentação.

## Visão geral

MyLib é uma aplicação self-hosted para organizar, enriquecer e reproduzir bibliotecas pessoais
de filmes e séries. O produto é entregue como um único servidor Rust: em produção, o bundle do
frontend é incorporado ao binário e servido pelo Axum.

Tecnologias principais:

- Backend: Rust 2024, Tokio, Axum, SQLx, SQLite/MySQL;
- Frontend: React, TypeScript, Vite, Material UI e TanStack Query;
- Reprodução: FFmpeg, FFprobe e HLS;
- Metadados: TMDB;
- Testes: testes nativos do Rust, Vitest, Testing Library e Playwright.

## Estrutura do repositório

```text
.
├── src/                    # backend Rust
│   ├── app/                # estado global, composição e middlewares
│   ├── core/               # configuração, erros e modelos compartilhados
│   ├── features/           # funcionalidades organizadas por domínio
│   │   ├── auth/
│   │   ├── catalog/        # catálogo, identificação, metadados e scanner
│   │   ├── libraries/      # bibliotecas e sincronização automática
│   │   ├── operations/     # atividade, métricas e saúde operacional
│   │   ├── playback/       # análise, sessões, HLS e progresso
│   │   └── recommendations/
│   ├── http/               # composição das rotas HTTP
│   ├── infrastructure/     # banco de dados e assets web incorporados
│   ├── bin/                # ferramentas e benchmarks opcionais
│   ├── lib.rs              # módulos públicos e exports de compatibilidade
│   └── main.rs             # ciclo de vida do processo
├── migrations/
│   ├── sqlite/             # migrations SQLite
│   └── mysql/              # migrations MySQL equivalentes
├── tests/                  # testes HTTP de integração do backend
├── web/                    # aplicação React
├── scripts/                # desenvolvimento, build e testes de carga
├── design/                 # referências do design system
├── docs/                   # documentação técnica
└── tools/                  # ferramentas locais, incluindo FFmpeg quando aplicável
```

Leia também `docs/ARCHITECTURE.md` e `CONTRIBUTING.md` antes de uma alteração estrutural.

## Arquitetura e dependências

A direção preferencial das dependências é:

```text
main -> app -> http -> features -> core
             |          |
             +----------+-> infrastructure
```

Regras:

1. `main.rs` deve cuidar apenas do processo: configuração, listener, shutdown e início de tarefas
   em segundo plano.
2. `app` cria o estado compartilhado e configura middlewares. Regras de negócio não pertencem a
   esse módulo.
3. `http` agrega routers. Cada handler deve permanecer na feature que é dona do comportamento.
4. `features` contém capacidades do produto. Código usado por apenas uma feature deve continuar
   dentro dela.
5. `core` contém somente contratos realmente compartilhados e estáveis. Não transformar `core`
   em uma pasta genérica para utilitários sem proprietário.
6. `infrastructure` contém adaptadores externos. Ela não deve decidir regras de negócio nem
   compor rotas.
7. Evitar dependências circulares entre features. Expor uma função ou tipo público pequeno quando
   uma integração entre domínios for necessária.
8. Os exports antigos em `src/lib.rs` preservam compatibilidade externa. Código interno novo deve
   preferir os caminhos canônicos, como `crate::features::playback` e
   `crate::infrastructure::database`.

## Onde colocar código novo

- Nova funcionalidade do usuário: `src/features/<dominio>/`;
- Nova rota: arquivo `api.rs` da feature proprietária e merge em `src/http/api.rs`;
- Regra ou caso de uso: `service.rs` dentro da feature;
- Consultas específicas de uma feature: `repository.rs` dentro da feature;
- Tipos locais: `models.rs` dentro da feature;
- Worker, processo ou comando externo: `runtime.rs` dentro da feature;
- Adaptador externo reutilizável: `src/infrastructure/`;
- Tipo compartilhado por vários domínios: `src/core/`, somente quando a dependência for estável;
- Executável auxiliar: `src/bin/`;
- Teste unitário: junto ao módulo em um bloco `#[cfg(test)]`;
- Teste de comportamento HTTP: `tests/`;
- Decisão arquitetural ou operação relevante: `docs/`.

Não criar novos arquivos `.rs` diretamente em `src/`, exceto `main.rs` e `lib.rs`.

## Padrões do backend Rust

- Usar Rust edition 2024 e respeitar a versão mínima definida em `Cargo.toml`.
- Formatar sempre com `cargo fmt`.
- O código deve passar no Clippy com warnings tratados como erros.
- Preferir tipos explícitos para estados e contratos; evitar valores mágicos espalhados.
- Usar `AppResult<T>` e `AppError` para erros retornados pela aplicação.
- Não usar `unwrap`, `expect` ou `panic!` em caminhos de produção quando o erro puder ser tratado.
- Não bloquear o runtime Tokio com I/O pesado. Usar APIs assíncronas ou `spawn_blocking` quando
  necessário.
- Manter handlers HTTP pequenos: extrair entrada, autorizar, chamar o caso de uso e mapear saída.
- Não expor diretamente tipos de persistência como contrato HTTP.
- Paginar listas potencialmente grandes e impor limites máximos no servidor.
- Preservar cancelamento, limites de concorrência e backpressure em scanner, metadados e playback.
- Usar `tracing` para observabilidade. Nunca registrar tokens, senhas, URLs com credenciais,
  chaves, conteúdo de arquivos pessoais ou outros segredos.
- Comentários devem explicar decisões e restrições, não repetir o código.
- Nomes de código, commits e identificadores técnicos devem ser claros e consistentes em inglês.
  Textos exibidos na interface devem usar o sistema de internacionalização.

## API HTTP

- Manter endpoints versionados sob `/api/v1`, exceto endpoints deliberadamente públicos como
  `/health`.
- Preservar o formato JSON existente e a convenção `camelCase`.
- Usar os códigos HTTP apropriados e erros estruturados por `AppError`.
- Toda rota privada deve validar autenticação, permissão e acesso à biblioteca quando aplicável.
- Alterações incompatíveis de contrato exigem uma nova versão da API ou uma estratégia explícita
  de migração.
- Nunca confiar em caminhos, IDs, nomes de arquivos ou headers fornecidos pelo cliente.
- Validar paginação, filtros, tamanhos e valores enumerados no servidor.
- Adicionar ou atualizar testes de integração ao criar ou modificar rotas.

## Persistência e migrations

- Toda mudança de schema deve criar a próxima migration numerada em `migrations/sqlite` e
  `migrations/mysql`.
- As duas migrations devem representar o mesmo modelo e comportamento, respeitando a sintaxe de
  cada banco.
- Nunca editar uma migration que possa ter sido executada por usuários. Criar uma nova versão.
- Migrations devem ser idempotentes conforme o mecanismo atual do projeto e seguras para dados
  existentes.
- Não remover, renomear ou reinterpretar dados sem uma estratégia explícita de migração.
- Consultas SQL devem utilizar bind parameters. Nunca interpolar entrada do usuário em SQL.
- SQLite continua sendo o ambiente padrão de desenvolvimento e testes; alterações também devem
  preservar compatibilidade conceitual com MySQL.
- Credenciais persistidas devem continuar criptografadas e nunca aparecer em respostas ou logs.

## Scanner, metadados e playback

- O scanner nunca deve alterar, mover ou apagar os arquivos de mídia do usuário.
- Diretórios inacessíveis devem gerar estado/aviso controlado, sem marcar incorretamente todo o
  conteúdo como removido.
- Testes não devem consultar o TMDB nem depender de uma biblioteca pessoal real.
- Chamadas externas devem ter timeout, concorrência limitada e tratamento de rate limit.
- Processos FFmpeg/FFprobe devem usar argumentos explícitos, caminhos validados e encerramento
  controlado.
- Novos pipelines de transcode devem alterar a chave de cache quando o formato de saída ou os
  parâmetros de compatibilidade mudarem.
- Servir somente playlists, segmentos e arquivos pertencentes à sessão autorizada.
- Validar compatibilidade de contêiner, vídeo, áudio e canais antes de escolher Direct Play,
  Direct Stream ou Transcode.
- Toda correção determinística de reprodução deve receber um teste de regressão quando possível.

## Padrões do frontend

- Manter TypeScript em modo estrito e não introduzir `any` sem justificativa documentada.
- Componentes reutilizáveis pertencem às áreas compartilhadas já existentes; comportamento de
  domínio deve permanecer em `web/src/features/<feature>`.
- Estado remoto deve usar TanStack Query. Zustand é reservado ao estado local/global de UI.
- Textos visíveis não devem ser espalhados como strings ad hoc quando pertencem ao i18n.
- Usar o tema e os tokens do Material UI; evitar cores e espaçamentos arbitrários.
- Preservar funcionamento nos temas claro e escuro, responsividade, foco por teclado e nomes
  acessíveis.
- Tratar estados de carregamento, vazio, erro e ausência de permissão.
- Não armazenar dados sensíveis em `localStorage` além do mecanismo de sessão já definido.
- Mudanças de comportamento devem incluir testes com Vitest/Testing Library quando aplicável.

## Segurança e privacidade

- Nunca adicionar credenciais, tokens, bancos, arquivos de mídia, conteúdo de `data/`, `.env` ou
  relatórios locais ao repositório.
- Não enfraquecer autenticação, autorização, CORS, headers de segurança ou proteção da última
  conta administrativa.
- Não revelar se um usuário existe durante login ou recuperação de acesso.
- Senhas devem continuar usando Argon2id e nunca devem ser serializadas.
- Caminhos locais são dados sensíveis: não retornar caminhos completos sem necessidade e não
  registrá-los em logs públicos.
- Toda entrada externa é não confiável, incluindo nomes TMDB, filenames, headers e parâmetros.
- Operações destrutivas precisam de escopo exato, confirmação quando aplicável e testes.

## Testes e critérios de conclusão

Antes de considerar uma alteração concluída, executar:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
npm --prefix web run lint
npm --prefix web run typecheck
npm --prefix web test -- --run
npm --prefix web run build
```

Para alterações somente de documentação, é suficiente validar formatação, links e comandos
afetados. Para alterações apenas no backend, os gates Rust são obrigatórios; ainda assim, execute
o typecheck/build web quando mudar contratos usados pelo frontend ou assets incorporados.

Uma mudança está pronta quando:

- o comportamento solicitado está implementado;
- os testes existentes continuam passando;
- há teste de regressão ou cobertura nova quando aplicável;
- Clippy, formatter, typecheck e build relevantes passam;
- migrations e documentação foram atualizadas;
- não há credenciais, artefatos gerados ou dados pessoais no diff;
- compatibilidade pública foi preservada ou a quebra foi explicitamente planejada.

## Git e pull requests

- Criar commits pequenos e coesos, com uma única intenção principal.
- Não misturar refatoração ampla com mudança funcional sem necessidade.
- Não sobrescrever mudanças de outros colaboradores nem limpar arquivos não relacionados.
- Não versionar `target/`, `data/`, `web/node_modules/`, `web/dist/`, bancos ou relatórios.
- O pull request deve explicar problema, solução, riscos, migrations e validação executada.
- Atualizar README, arquitetura e exemplos quando uma mudança tornar a documentação incorreta.
- Manter compatibilidade com Windows e Linux em scripts, caminhos e execução de processos.

## Princípios de manutenção

- Clareza e previsibilidade são mais importantes que abstrações prematuras.
- Uma feature deve ter um proprietário estrutural evidente.
- Código compartilhado precisa ser compartilhado de verdade; caso contrário, permanece local.
- Preferir mudanças incrementais que mantenham o sistema executável e testável.
- Corrigir a causa raiz e preservar diagnósticos úteis para operação self-hosted.
- Não alterar comportamento existente silenciosamente durante uma reorganização.
