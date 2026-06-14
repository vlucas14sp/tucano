# Tucano

Navegador web nativo para **GNOME / Fedora**, escrito em **Rust**.
Usa o motor **WebKitGTK** (família Safari/WebKit) com **GTK4 + libadwaita**.

## Funcionalidades

- Abas (`AdwTabView` / `AdwTabBar`)
- **Nova aba estilo Arc**: abre uma página de início com uma busca centralizada;
  a página só carrega depois que você digita a URL ou a pesquisa
- Navegação: voltar, avançar, recarregar
- Barra de endereço inteligente: detecta URL ou pesquisa no buscador
- Indicador de progresso de carregamento na barra de endereço
- Título e spinner por aba
- Atalhos de teclado

## Atalhos de teclado

| Atalho        | Ação                          |
|---------------|-------------------------------|
| `Ctrl+T`      | Nova aba                      |
| `Ctrl+W`      | Fechar aba                    |
| `Ctrl+L`      | Focar a barra de endereço     |
| `Ctrl+R` / `F5` | Recarregar                  |
| `Alt+←`       | Voltar                        |
| `Alt+→`       | Avançar                       |

## Dependências (Fedora)

```bash
sudo dnf install -y rust cargo gtk4-devel libadwaita-devel webkitgtk6.0-devel
```

## Compilar e executar

```bash
cargo run --release
```

## Estrutura

| Arquivo          | Papel                                              |
|------------------|----------------------------------------------------|
| `src/main.rs`    | Ponto de entrada; cria o `adw::Application`         |
| `src/browser.rs` | Janela, abas, navegação e lógica da barra de URL    |

## Próximos passos sugeridos

- Histórico e favoritos persistentes (ex.: SQLite via `rusqlite`)
- Downloads (`WebView::connect_download_started`)
- Atalhos de teclado (Ctrl+T, Ctrl+W, Ctrl+L, Ctrl+R)
- Menu de aplicativo e modo privado (`WebContext` efêmero)
