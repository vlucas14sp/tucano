# Tucano

Navegador web nativo para **GNOME / Fedora**, escrito em **Rust**.
Usa o motor **WebKitGTK** (família Safari/WebKit) com **GTK4 + libadwaita**.

## Funcionalidades

- Abas (`AdwTabView`) com **roda radial estilo "seleção de armas"**: a barra de
  abas fica oculta; um botão flutuante de tucano (centro-esquerda) revela meia-roda
  de abas que gira com o scroll do mouse, selecionando a aba central ao vivo
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
| `Ctrl+E`      | Abre/fecha a roda de abas (depois role o mouse para girar) |
| `Ctrl+Tab`    | Aciona a roda e avança uma aba; solte o `Ctrl` para confirmar |
| `Ctrl+Shift+Tab` | Aciona a roda e volta uma aba              |

## Dependências (Fedora)

```bash
sudo dnf install -y rust cargo gtk4-devel libadwaita-devel webkitgtk6.0-devel
```

## Compilar e executar

```bash
cargo run --release
```

`TUCANO_URL=<url> cargo run` abre direto numa página (útil para depurar).

### Reprodução de vídeo (opcional)

A renderização das páginas usa só o WebKitGTK, mas para **tocar vídeos**
(YouTube etc.) o WebKitGTK depende dos plugins do GStreamer. Para cobertura
completa de codecs, instale (via RPM Fusion):

```bash
sudo dnf install -y gstreamer1-libav gstreamer1-plugins-good gstreamer1-plugins-bad-free
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
