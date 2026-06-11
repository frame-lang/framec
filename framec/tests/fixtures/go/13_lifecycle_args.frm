@@system LifecycleArgs {
    interface:
        load(n: int, label: string)
        total(): int
        tag(): string

    machine:
        $Idle {
            load(n: int, label: string) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: int, name: string) {
                @@:self.sum = count + 1;
                @@:self.label = name;
            }
            total(): int {
                @@:(@@:self.sum)
                return
            }
            tag(): string {
                @@:(@@:self.label)
                return
            }
        }

    domain:
        sum: int = 0
        label: string = ""
}
