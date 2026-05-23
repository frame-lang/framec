@@system StateArgs {
    interface:
        load(initial: Integer)
        adjust(delta: Integer)
        peek(): Integer

    machine:
        $Idle {
            load(initial: Integer) { -> $Holding(initial) }
        }

        $Holding(value: Integer) {
            adjust(delta: Integer) { -> $Holding(value + delta) }
            peek(): Integer { @@:(value) }
        }
}
