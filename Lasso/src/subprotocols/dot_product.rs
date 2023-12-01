use super::bullet::BulletReductionProof;
use crate::poly::commitments::{Commitments, MultiCommitGens};
use crate::utils::errors::ProofVerifyError;
use crate::utils::math::Math;
use crate::utils::random::RandomTape;
use crate::utils::transcript::ProofTranscript;
use ark_ec::CurveGroup;
use ark_serialize::*;
use merlin::Transcript;

#[derive(Debug, CanonicalSerialize, CanonicalDeserialize)]
pub struct DotProductProof<G: CurveGroup> {
  delta: G,
  beta: G,
  z: Vec<G::ScalarField>,
  z_delta: G::ScalarField,
  z_beta: G::ScalarField,
}

impl<G: CurveGroup> DotProductProof<G> {
  fn protocol_name() -> &'static [u8] {
    b"dot product proof"
  }

  pub fn compute_dotproduct(a: &[G::ScalarField], b: &[G::ScalarField]) -> G::ScalarField {
    assert_eq!(a.len(), b.len());
    (0..a.len()).map(|i| a[i] * b[i]).sum()
  }

  #[allow(dead_code)]
  pub fn prove(
    gens_1: &MultiCommitGens<G>,
    gens_n: &MultiCommitGens<G>,
    transcript: &mut Transcript,
    random_tape: &mut RandomTape<G>,
    x_vec: &[G::ScalarField],
    blind_x: &G::ScalarField,
    a_vec: &[G::ScalarField],
    y: &G::ScalarField,
    blind_y: &G::ScalarField,
  ) -> (Self, G, G) {
    <Transcript as ProofTranscript<G>>::append_protocol_name(
      transcript,
      DotProductProof::<G>::protocol_name(),
    );

    let n = x_vec.len();
    assert_eq!(x_vec.len(), a_vec.len());
    assert_eq!(gens_n.n, a_vec.len());
    assert_eq!(gens_1.n, 1);

    // produce randomness for the proofs
    let d_vec = random_tape.random_vector(b"d_vec", n);
    let r_delta = random_tape.random_scalar(b"r_delta");
    let r_beta = random_tape.random_scalar(b"r_beta");

    let Cx = Commitments::batch_commit(x_vec, blind_x, gens_n);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cx", &Cx);

    let Cy = y.commit(blind_y, gens_1);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cy", &Cy);

    <Transcript as ProofTranscript<G>>::append_scalars(transcript, b"a", a_vec);

    let delta = Commitments::batch_commit(&d_vec, &r_delta, gens_n);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"delta", &delta);

    let dotproduct_a_d = DotProductProof::<G>::compute_dotproduct(a_vec, &d_vec);

    let beta = dotproduct_a_d.commit(&r_beta, gens_1);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"beta", &beta);

    let c = <Transcript as ProofTranscript<G>>::challenge_scalar(transcript, b"c");

    let z = (0..d_vec.len())
      .map(|i| c * x_vec[i] + d_vec[i])
      .collect::<Vec<G::ScalarField>>();

    let z_delta = c * blind_x + r_delta;
    let z_beta = c * blind_y + r_beta;

    (
      DotProductProof {
        delta,
        beta,
        z,
        z_delta,
        z_beta,
      },
      Cx,
      Cy,
    )
  }

  pub fn verify(
    &self,
    gens_1: &MultiCommitGens<G>,
    gens_n: &MultiCommitGens<G>,
    transcript: &mut Transcript,
    a: &[G::ScalarField],
    Cx: &G,
    Cy: &G,
  ) -> Result<(), ProofVerifyError> {
    if a.len() != gens_n.n {
      return Err(ProofVerifyError::InvalidInputLength(gens_n.n, a.len()));
    }
    if gens_1.n != 1 {
      return Err(ProofVerifyError::InvalidInputLength(1, gens_1.n));
    }

    <Transcript as ProofTranscript<G>>::append_protocol_name(
      transcript,
      DotProductProof::<G>::protocol_name(),
    );

    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cx", Cx);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cy", Cy);

    <Transcript as ProofTranscript<G>>::append_scalars(transcript, b"a", a);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"delta", &self.delta);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"beta", &self.beta);

    let c = <Transcript as ProofTranscript<G>>::challenge_scalar(transcript, b"c");

    let mut result =
      *Cx * c + self.delta == Commitments::batch_commit(self.z.as_ref(), &self.z_delta, gens_n);

    let dotproduct_z_a = DotProductProof::<G>::compute_dotproduct(&self.z, a);
    result &= *Cy * c + self.beta == dotproduct_z_a.commit(&self.z_beta, gens_1);

    if result {
      Ok(())
    } else {
      Err(ProofVerifyError::InternalError)
    }
  }
}

pub struct DotProductProofGens<G> {
  n: usize,
  pub gens_n: MultiCommitGens<G>,
  pub gens_1: MultiCommitGens<G>,
}

impl<G: CurveGroup> DotProductProofGens<G> {
  pub fn new(n: usize, label: &[u8]) -> Self {
    let (gens_n, gens_1) = MultiCommitGens::new(n + 1, label).split_at(n);
    DotProductProofGens { n, gens_n, gens_1 }
  }
}

#[derive(Debug, CanonicalSerialize, CanonicalDeserialize)]
pub struct DotProductProofLog<G: CurveGroup> {
  bullet_reduction_proof: BulletReductionProof<G>,
  delta: G,
  beta: G,
  z1: G::ScalarField,
  z2: G::ScalarField,
}

impl<G: CurveGroup> DotProductProofLog<G> {
  fn protocol_name() -> &'static [u8] {
    b"dot product proof (log)"
  }

  #[tracing::instrument(skip_all, name = "DotProductProofLog.prove")]
  pub fn prove(
    gens: &DotProductProofGens<G>,
    transcript: &mut Transcript,
    random_tape: &mut RandomTape<G>,
    x_vec: &[G::ScalarField],
    blind_x: &G::ScalarField,
    a_vec: &[G::ScalarField],
    y: &G::ScalarField,
    blind_y: &G::ScalarField,
  ) -> (Self, G, G) {
    <Transcript as ProofTranscript<G>>::append_protocol_name(
      transcript,
      DotProductProofLog::<G>::protocol_name(),
    );

    let n = x_vec.len();
    assert_eq!(x_vec.len(), a_vec.len());
    assert_eq!(gens.n, n);

    // produce randomness for generating a proof
    let d = random_tape.random_scalar(b"d");
    let r_delta = random_tape.random_scalar(b"r_delta");
    let r_beta = random_tape.random_scalar(b"r_delta");
    let blinds_vec = {
      let v1 = random_tape.random_vector(b"blinds_vec_1", 2 * n.log_2());
      let v2 = random_tape.random_vector(b"blinds_vec_2", 2 * n.log_2());
      (0..v1.len())
        .map(|i| (v1[i], v2[i]))
        .collect::<Vec<(G::ScalarField, G::ScalarField)>>()
    };

    let Cx = Commitments::batch_commit(x_vec, blind_x, &gens.gens_n);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cx", &Cx);

    let Cy = y.commit(blind_y, &gens.gens_1);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cy", &Cy);

    <Transcript as ProofTranscript<G>>::append_scalars(transcript, b"a", a_vec);

    let blind_Gamma = *blind_x + *blind_y;
    let (bullet_reduction_proof, _Gamma_hat, x_hat, a_hat, g_hat, rhat_Gamma) =
      BulletReductionProof::prove(
        transcript,
        &gens.gens_1.G[0],
        &gens.gens_n.G,
        &gens.gens_n.h,
        x_vec,
        a_vec,
        &blind_Gamma,
        &blinds_vec,
      );
    let y_hat = x_hat * a_hat;

    let delta = {
      let gens_hat = MultiCommitGens {
        n: 1,
        G: vec![g_hat],
        h: gens.gens_1.h,
      };
      d.commit(&r_delta, &gens_hat)
    };
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"delta", &delta);

    let beta = d.commit(&r_beta, &gens.gens_1);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"beta", &beta);

    let c = <Transcript as ProofTranscript<G>>::challenge_scalar(transcript, b"c");

    let z1 = d + c * y_hat;
    let z2 = a_hat * (c * rhat_Gamma + r_beta) + r_delta;

    (
      DotProductProofLog {
        bullet_reduction_proof,
        delta,
        beta,
        z1,
        z2,
      },
      Cx,
      Cy,
    )
  }

  pub fn verify(
    &self,
    n: usize,
    gens: &DotProductProofGens<G>,
    transcript: &mut Transcript,
    a: &[G::ScalarField],
    Cx: &G,
    Cy: &G,
  ) -> Result<(), ProofVerifyError> {
    assert_eq!(gens.n, n);
    assert_eq!(a.len(), n);

    <Transcript as ProofTranscript<G>>::append_protocol_name(
      transcript,
      DotProductProofLog::<G>::protocol_name(),
    );
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cx", Cx);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"Cy", Cy);
    <Transcript as ProofTranscript<G>>::append_scalars(transcript, b"a", a);

    let Gamma = *Cx + *Cy;

    let (g_hat, Gamma_hat, a_hat) =
      self
        .bullet_reduction_proof
        .verify(n, a, transcript, &Gamma, &gens.gens_n.G)?;

    <Transcript as ProofTranscript<G>>::append_point(transcript, b"delta", &self.delta);
    <Transcript as ProofTranscript<G>>::append_point(transcript, b"beta", &self.beta);

    let c = <Transcript as ProofTranscript<G>>::challenge_scalar(transcript, b"c");

    let c_s = &c;
    let beta_s = self.beta;
    let a_hat_s = &a_hat;
    let delta_s = self.delta;
    let z1_s = &self.z1;
    let z2_s = &self.z2;

    let lhs = (Gamma_hat * c_s + beta_s) * a_hat_s + delta_s;
    let rhs = (g_hat + gens.gens_1.G[0] * a_hat_s) * z1_s + gens.gens_1.h * z2_s;

    (lhs == rhs)
      .then_some(())
      .ok_or(ProofVerifyError::InternalError)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use ark_curve25519::EdwardsProjective as G1Projective;
  use ark_std::test_rng;
  use ark_std::UniformRand;

  #[test]
  fn check_dotproductproof() {
    check_dotproductproof_helper::<G1Projective>()
  }

  fn check_dotproductproof_helper<G: CurveGroup>() {
    let mut prng = test_rng();

    let n = 1024;

    let gens_1 = MultiCommitGens::<G>::new(1, b"test-two");
    let gens_1024 = MultiCommitGens::new(n, b"test-1024");

    let mut x: Vec<G::ScalarField> = Vec::new();
    let mut a: Vec<G::ScalarField> = Vec::new();
    for _ in 0..n {
      x.push(G::ScalarField::rand(&mut prng));
      a.push(G::ScalarField::rand(&mut prng));
    }
    let y = DotProductProof::<G>::compute_dotproduct(&x, &a);
    let r_x = G::ScalarField::rand(&mut prng);
    let r_y = G::ScalarField::rand(&mut prng);

    let mut random_tape = RandomTape::new(b"proof");
    let mut prover_transcript = Transcript::new(b"example");
    let (proof, Cx, Cy) = DotProductProof::prove(
      &gens_1,
      &gens_1024,
      &mut prover_transcript,
      &mut random_tape,
      &x,
      &r_x,
      &a,
      &y,
      &r_y,
    );

    let mut verifier_transcript = Transcript::new(b"example");
    assert!(proof
      .verify(&gens_1, &gens_1024, &mut verifier_transcript, &a, &Cx, &Cy)
      .is_ok());
  }

  #[test]
  fn check_dotproductproof_log() {
    check_dotproductproof_log_helper::<G1Projective>()
  }
  fn check_dotproductproof_log_helper<G: CurveGroup>() {
    let mut prng = test_rng();

    let n = 1024;

    let gens = DotProductProofGens::<G>::new(n, b"test-1024");

    let x: Vec<G::ScalarField> = (0..n).map(|_i| G::ScalarField::rand(&mut prng)).collect();
    let a: Vec<G::ScalarField> = (0..n).map(|_i| G::ScalarField::rand(&mut prng)).collect();
    let y = DotProductProof::<G>::compute_dotproduct(&x, &a);

    let r_x = G::ScalarField::rand(&mut prng);
    let r_y = G::ScalarField::rand(&mut prng);

    let mut random_tape = RandomTape::new(b"proof");
    let mut prover_transcript = Transcript::new(b"example");
    let (proof, Cx, Cy) = DotProductProofLog::prove(
      &gens,
      &mut prover_transcript,
      &mut random_tape,
      &x,
      &r_x,
      &a,
      &y,
      &r_y,
    );

    let mut verifier_transcript = Transcript::new(b"example");
    assert!(proof
      .verify(n, &gens, &mut verifier_transcript, &a, &Cx, &Cy)
      .is_ok());
  }
}
