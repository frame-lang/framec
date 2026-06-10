@@system LinearFsm {
    interface:
        start()
        progress(amount: Int)
        finish()

    machine:
        $Idle {
            start() { -> $Active }
        }

        $Active {
            progress(amount: Int) {
                @@:self.total = @@:self.total + amount
            }
            finish() { -> $Done }
        }

        $Done { }

    domain:
        total: Int = 0
}
