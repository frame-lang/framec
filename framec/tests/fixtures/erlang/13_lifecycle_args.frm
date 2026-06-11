@@system LifecycleArgs {
    interface:
        load(n: integer, label: string)
        total(): integer
        tag(): string

    machine:
        $Idle {
            load(n: integer, label: string) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: integer, name: string) {
                @@:self.sum = count + 1;
                @@:self.label = name;
            }
            total(): integer {
                @@:(@@:self.sum)
                return
            }
            tag(): string {
                @@:(@@:self.label)
                return
            }
        }

    domain:
        sum: integer = 0
        label: string = ""
}
