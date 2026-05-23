@@system StateArgs {
    interface:
        load(initial: Int)
        adjust(delta: Int)
        peek(): Int

    machine:
        $Idle {
            load(initial: Int) { -> $Holding(initial) }
        }

        $Holding(value: Int) {
            adjust(delta: Int) { -> $Holding(value + delta) }
            peek(): Int { @@:(value) }
        }
}
