package main

@@system StateArgs {
    interface:
        load(initial: int)
        adjust(delta: int)
        peek(): int

    machine:
        $Idle {
            load(initial: int) { -> $Holding(initial) }
        }

        $Holding(value: int) {
            adjust(delta: int) { -> $Holding(value + delta) }
            peek(): int { @@:(value) }
        }
}