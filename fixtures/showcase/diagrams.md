# Diagrams

## flowchart

```mermaid
graph TD
    A[Start] --> B{Decision}
    B -->|Yes| C[Do the thing]
    B -->|No| D[Stop]
    C --> D
```

## sequence

```mermaid
sequenceDiagram
    participant Alice
    participant Bob
    Alice->>Bob: Hello Bob
    Bob-->>Alice: Hi Alice
    Alice->>Bob: How are you?
    Bob-->>Alice: Fine
```

## class

```mermaid
classDiagram
    Animal <|-- Duck
    Animal <|-- Fish
    Animal : +int age
    Animal : +isMammal()
    Duck : +String beakColor
    Duck : +swim()
    Fish : -int sizeInFeet
    Fish : +canEat()
```

## state

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Active : start
    Active --> Idle : stop
    Active --> [*] : finish
```

## er

```mermaid
erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains
    CUSTOMER {
        string name
        string email
    }
    ORDER {
        int id
        string status
    }
```

## journey

```mermaid
journey
    title A writing day
    section Morning
      Open the file: 5: Author
      Draft a section: 3: Author
    section Afternoon
      Render the PDF: 4: Author
      Fix a diagram: 2: Author
```

## gantt

```mermaid
gantt
    title Project plan
    dateFormat YYYY-MM-DD
    section Design
    Spec     :a1, 2026-01-01, 14d
    section Build
    Engine   :after a1, 21d
    Preview  :2026-02-10, 14d
```

## pie

```mermaid
pie title Pets adopted
    "Dogs" : 42
    "Cats" : 33
    "Birds" : 15
    "Other" : 10
```

## quadrant

```mermaid
quadrantChart
    title Reach and engagement
    x-axis Low Reach --> High Reach
    y-axis Low Engagement --> High Engagement
    quadrant-1 Stars
    quadrant-2 Questions
    quadrant-3 Dogs
    quadrant-4 Cash cows
    Campaign A: [0.35, 0.62]
    Campaign B: [0.72, 0.28]
    Campaign C: [0.55, 0.80]
```

## mindmap

```mermaid
mindmap
  root((Markloom))
    Parse
      Blocks
      Inlines
    Style
      Presets
    Typeset
      Preview
      PDF
```

## timeline

```mermaid
timeline
    title Markloom history
    2024 : Idea
    2025 : Prototype
    2026 : Engine
         : Preview
```

## gitgraph

```mermaid
gitGraph
    commit id: "init"
    branch develop
    checkout develop
    commit id: "feat"
    checkout main
    merge develop
    commit id: "release"
```

## xychart

```mermaid
xychart-beta
    title "Revenue"
    x-axis [jan, feb, mar, apr, may]
    y-axis "USD" 0 --> 100
    bar [20, 50, 40, 80, 65]
    line [20, 50, 40, 80, 65]
```
