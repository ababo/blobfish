CREATE EXTENSION pgcrypto;

CREATE TABLE "user"(
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  created_at timestamp with time zone NOT NULL DEFAULT now(),
  balance decimal NOT NULL DEFAULT 0,
  allocated_fee decimal NOT NULL DEFAULT 0
);

CREATE INDEX user_allocated_fee_idx ON "user"(allocated_fee)
WHERE
  allocated_fee > 0;

CREATE TABLE token(
  id uuid PRIMARY KEY,
  hash text NOT NULL,
  created_at timestamp with time zone NOT NULL DEFAULT now(),
  "user" uuid NOT NULL,
  is_admin boolean NOT NULL,
  FOREIGN KEY("user") REFERENCES "user"(id)
);

CREATE INDEX token_user_idx ON token("user");

CREATE TABLE capability(
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  name text NOT NULL,
  compute_load integer NOT NULL,
  memory_load integer NOT NULL,
  fee decimal NOT NULL,
  languages text
);

CREATE TYPE task_type AS ENUM('segment', 'transcribe');

CREATE TABLE task_type_tariff_capability(
  task_type task_type NOT NULL,
  tariff text NOT NULL,
  capability uuid NOT NULL,
  FOREIGN KEY(capability) REFERENCES capability(id)
);

CREATE TABLE node(
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  label text NOT NULL,
  ip_address inet NOT NULL,
  compute_capacity integer NOT NULL,
  memory_capacity integer NOT NULL,
  compute_load integer NOT NULL DEFAULT 0,
  memory_load integer NOT NULL DEFAULT 0
);

CREATE INDEX node_avail_compute_idx ON node((compute_capacity - compute_load));

CREATE INDEX node_avail_memory_idx ON node((memory_capacity - memory_load));

CREATE TABLE node_capability(
  node uuid NOT NULL,
  capability uuid NOT NULL,
  FOREIGN KEY(node) REFERENCES node(id),
  FOREIGN KEY(capability) REFERENCES capability(id)
);

CREATE INDEX node_capability_node_idx ON node_capability(node);

CREATE TYPE payment_processor AS ENUM('paypal');

CREATE TYPE payment_status AS ENUM('new', 'completed', 'canceled');

CREATE TABLE payment(
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  created_at timestamp with time zone NOT NULL DEFAULT now(),
  status payment_status NOT NULL,
  currency text NOT NULL,
  amount decimal NOT NULL,
  from_user uuid NOT NULL,
  to_user uuid NOT NULL,
  processor payment_processor NOT NULL,
  reference text NOT NULL,
  details text,
  FOREIGN KEY(from_user) REFERENCES "user"(id),
  FOREIGN KEY(to_user) REFERENCES "user"(id)
);

CREATE INDEX payment_from_user_idx ON payment(from_user);

CREATE INDEX payment_reference_idx ON payment(reference);

INSERT INTO
  "user"
VALUES
  (
    '61abe888-3947-4dc6-9db7-ede01a1618e2',
    '2024-05-22T17:33:00',
    10,
    0
  );

INSERT INTO
  token
VALUES
  (
    -- Authorization: Bearer QKvO9M1eSniqWjAsQQO9snP2IWWsggdV0l8/jCqgATpOyYUZpuAcOjyt8YJcKjxN
    '40abcef4-cd5e-4a78-aa5a-302c4103bdb2',
    '$2a$06$7CYjgqKs8AAjJ4aSrDhOvucRa07XM4HbyEzky7l4KbG2e.MNVDxOy',
    '2024-05-22T17:33:00',
    '61abe888-3947-4dc6-9db7-ede01a1618e2',
    'true'
  );

INSERT INTO
  capability
VALUES
  (
    '14150c46-5d42-482c-a9aa-dbb4d4885d1d',
    'segment-cpu',
    20,
    20,
    0.000007,
    NULL
  );

INSERT INTO
  capability
VALUES
  (
    'f79e4a21-0d6f-4e65-8a67-49c07b61f337',
    'transcribe-small-cpu',
    70,
    50,
    0.000026,
    'af,am,ar,as,az,ba,be,bg,bn,bo,br,bs,ca,cs,cy,da,de,el,en,es,et,eu,fa,fi,fo,fr,gl,gu,ha,haw,he,hi,hr,ht,hu,hy,id,is,it,ja,jw,ka,kk,km,kn,ko,la,lb,ln,lo,lt,lv,mg,mi,mk,ml,mn,mr,ms,mt,my,ne,nl,nn,no,oc,pa,pl,ps,pt,ro,ru,sa,sd,si,sk,sl,sn,so,sq,sr,su,sv,sw,ta,te,tg,th,tk,tl,tr,tt,uk,ur,uz,vi,yi,yo,zh,yue'
  );

INSERT INTO
  task_type_tariff_capability
VALUES
  (
    'segment',
    'basic',
    '14150c46-5d42-482c-a9aa-dbb4d4885d1d'
  );

INSERT INTO
  task_type_tariff_capability
VALUES
  (
    'transcribe',
    'basic',
    'f79e4a21-0d6f-4e65-8a67-49c07b61f337'
  );

INSERT INTO
  node
VALUES
  (
    '3c3de81f-4b20-4cb4-90bd-913a61a8c7b5',
    'test',
    '127.0.0.1',
    90,
    70,
    0,
    0
  );

INSERT INTO
  node_capability
VALUES
  (
    '3c3de81f-4b20-4cb4-90bd-913a61a8c7b5',
    '14150c46-5d42-482c-a9aa-dbb4d4885d1d'
  );

INSERT INTO
  node_capability
VALUES
  (
    '3c3de81f-4b20-4cb4-90bd-913a61a8c7b5',
    'f79e4a21-0d6f-4e65-8a67-49c07b61f337'
  );
