@@system StateArgs {
    interface:
        load(initial: number)
        adjust(delta: number)
        peek(): number

    machine:
        $Idle {
            load(initial: number) { -> $Holding(initial) }
        }

        $Holding(value: number) {
            adjust(delta: number) { -> $Holding(value + delta) }
            peek(): number { @@:(value) }
        }
}
