@@system LinearFsm {
    interface:
        start()
        progress(amount: number)
        finish()

    machine:
        $Idle {
            start() { -> $Active }
        }

        $Active {
            progress(amount: number) {
                @@:self.total = @@:self.total + amount
            }
            finish() { -> $Done }
        }

        $Done { }

    domain:
        total: number = 0
}
