@@system StateArgs {
    interface:
        load(initial: integer)
        adjust(delta: integer)
        peek(): integer

    machine:
        $Idle {
            load(initial: integer) { -> $Holding(initial) }
        }

        $Holding(value: integer) {
            adjust(delta: integer) { -> $Holding(value + delta) }
            peek(): integer { @@:(value) }
        }
}
