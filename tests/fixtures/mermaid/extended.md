# Extended Mermaid diagrams

## Gantt release plan

```mermaid
gantt
    title Release plan
    dateFormat YYYY-MM-DD
    excludes weekends
    section Development
    Implement :done, impl, 2026-10-01, 3d
    Test :active, test, after impl, 2d
    Ship :milestone, after test, 0d
```

## Timeline

```mermaid
timeline
    title MD Star releases
    2025 : Prototype
    2026 : Native reader : Editor
```

## Mindmap

```mermaid
mindmap
  root((MD Star))
    Reader
      Markdown
      Diagrams
    Editor
      Live preview
```

## User journey

```mermaid
journey
    title Ship a release
    section Development
      Code: 5: Developer
      Test: 4: Developer
    section Distribution
      Install: 5: Reader
```
